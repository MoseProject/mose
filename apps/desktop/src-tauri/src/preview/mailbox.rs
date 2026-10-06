//! Bounded single-consumer mailbox with producer-side replacement of obsolete payloads.
use crate::state::lock_or_recover;
use std::collections::VecDeque;
use std::sync::mpsc::{RecvError, RecvTimeoutError, SendError, TryRecvError};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
struct State<T> {
    queue: VecDeque<T>,
    senders: usize,
    receiver: bool,
}
struct Shared<T> {
    state: Mutex<State<T>>,
    wake: Condvar,
    space: Condvar,
    capacity: usize,
    merge: fn(&mut VecDeque<T>, T) -> Option<T>,
}
pub(crate) struct Sender<T>(Arc<Shared<T>>);
pub(crate) struct Receiver<T>(Arc<Shared<T>>);
pub(crate) fn channel<T>(
    capacity: usize,
    merge: fn(&mut VecDeque<T>, T) -> Option<T>,
) -> (Sender<T>, Receiver<T>) {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            queue: VecDeque::new(),
            senders: 1,
            receiver: true,
        }),
        wake: Condvar::new(),
        space: Condvar::new(),
        capacity,
        merge,
    });
    (Sender(shared.clone()), Receiver(shared))
}
impl<T> Sender<T> {
    pub fn send(&self, value: T) -> Result<(), SendError<T>> {
        let mut state = lock_or_recover(&self.0.state);
        if !state.receiver {
            return Err(SendError(value));
        }
        let Some(value) = (self.0.merge)(&mut state.queue, value) else {
            self.0.wake.notify_one();
            return Ok(());
        };
        while state.queue.len() >= self.0.capacity && state.receiver {
            state = self
                .0
                .space
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if !state.receiver {
            return Err(SendError(value));
        }
        state.queue.push_back(value);
        self.0.wake.notify_one();
        Ok(())
    }
}
impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        lock_or_recover(&self.0.state).senders -= 1;
        self.0.wake.notify_all();
    }
}
impl<T> Receiver<T> {
    pub fn try_recv(&self) -> Result<T, TryRecvError> {
        let mut state = lock_or_recover(&self.0.state);
        if let Some(value) = state.queue.pop_front() {
            self.0.space.notify_one();
            return Ok(value);
        }
        Err(if state.senders == 0 {
            TryRecvError::Disconnected
        } else {
            TryRecvError::Empty
        })
    }
    pub fn recv_timeout(&self, timeout: Duration) -> Result<T, RecvTimeoutError> {
        let until = Instant::now() + timeout;
        let mut state = lock_or_recover(&self.0.state);
        loop {
            if let Some(value) = state.queue.pop_front() {
                self.0.space.notify_one();
                return Ok(value);
            }
            if state.senders == 0 {
                return Err(RecvTimeoutError::Disconnected);
            }
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(RecvTimeoutError::Timeout);
            }
            state = self
                .0
                .wake
                .wait_timeout(state, left)
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .0;
        }
    }
    pub fn recv(&self) -> Result<T, RecvError> {
        loop {
            match self.recv_timeout(Duration::from_secs(30)) {
                Ok(v) => return Ok(v),
                Err(RecvTimeoutError::Disconnected) => return Err(RecvError),
                Err(_) => {}
            }
        }
    }
    pub fn try_iter(&self) -> impl Iterator<Item = T> + '_ {
        std::iter::from_fn(|| self.try_recv().ok())
    }
}
impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let mut state = lock_or_recover(&self.0.state);
        state.receiver = false;
        state.queue.clear();
        self.0.space.notify_all();
    }
}

#[cfg(test)]
#[path = "mailbox_test.rs"]
mod tests;
