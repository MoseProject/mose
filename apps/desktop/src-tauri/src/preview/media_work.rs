//! Bounded admission for media I/O. Interactive work has reserved workers so imports cannot
//! occupy every decoder slot. Limits are safety policies, not measured optimal settings.
use crate::state::lock_or_recover;
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};
type Job = Box<dyn FnOnce() + Send>;
struct Queue {
    interactive: VecDeque<Job>,
    background: VecDeque<Job>,
    closed: bool,
}
struct Shared {
    queue: Mutex<Queue>,
    wake: Condvar,
}
pub struct MediaWork {
    shared: Arc<Shared>,
}
impl Default for MediaWork {
    fn default() -> Self {
        let shared = Arc::new(Shared {
            queue: Mutex::new(Queue {
                interactive: VecDeque::new(),
                background: VecDeque::new(),
                closed: false,
            }),
            wake: Condvar::new(),
        });
        for interactive in [true, true, false, false] {
            let shared = shared.clone();
            std::thread::spawn(move || loop {
                let job = {
                    let mut queue = lock_or_recover(&shared.queue);
                    loop {
                        if queue.closed {
                            return;
                        }
                        let jobs = if interactive {
                            &mut queue.interactive
                        } else {
                            &mut queue.background
                        };
                        if let Some(job) = jobs.pop_front() {
                            break job;
                        }
                        queue = shared
                            .wake
                            .wait(queue)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                };
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(job)).is_err() {
                    tracing::error!("media worker recovered after job panic");
                }
            });
        }
        Self { shared }
    }
}
impl MediaWork {
    pub fn submit(&self, interactive: bool, job: impl FnOnce() + Send + 'static) -> bool {
        let mut queue = lock_or_recover(&self.shared.queue);
        if queue.closed {
            return false;
        }
        let jobs = if interactive {
            &mut queue.interactive
        } else {
            &mut queue.background
        };
        if jobs.len() >= 64 {
            return false;
        }
        jobs.push_back(Box::new(job));
        self.shared.wake.notify_all();
        true
    }
}
impl Drop for MediaWork {
    fn drop(&mut self) {
        let mut queue = lock_or_recover(&self.shared.queue);
        queue.closed = true;
        queue.background.clear();
        queue.interactive.clear();
        self.shared.wake.notify_all();
    }
}
