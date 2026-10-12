//! How many imports may run their heavy steps at once. Every imported file gets its own thread,
//! and the steps after the probe each keep the machine's cores busy: a proxy encode is bound by
//! decoding, so N of them at once all finish together after N times as long, and nothing can be
//! used until then. A gate lets a few run at a time, in the order the files were imported, so the
//! first proxies are ready early and the rest follow.

use std::collections::VecDeque;
use std::sync::{Condvar, Mutex, OnceLock};

use crate::state::lock_or_recover;

/// A counting gate that lets `limit` holders in at once and admits the rest first come, first
/// served.
pub struct Gate {
    limit: usize,
    state: Mutex<State>,
    changed: Condvar,
}

#[derive(Default)]
struct State {
    running: usize,
    waiting: VecDeque<u64>,
    next_ticket: u64,
}

/// A place in the gate, given back when dropped.
pub struct Permit<'a> {
    gate: &'a Gate,
}

impl Gate {
    pub const fn new(limit: usize) -> Self {
        Self {
            limit,
            state: Mutex::new(State {
                running: 0,
                waiting: VecDeque::new(),
                next_ticket: 0,
            }),
            changed: Condvar::new(),
        }
    }

    /// Waits for a place. Never less than one holder may be in, whatever the limit says.
    pub fn enter(&self) -> Permit<'_> {
        let mut state = lock_or_recover(&self.state);
        let ticket = state.next_ticket;
        state.next_ticket += 1;
        state.waiting.push_back(ticket);
        while !(state.running < self.limit.max(1) && state.waiting.front() == Some(&ticket)) {
            state = self
                .changed
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        state.waiting.pop_front();
        state.running += 1;
        // The next in line may fit too.
        self.changed.notify_all();
        Permit { gate: self }
    }

    #[cfg(test)]
    pub fn running(&self) -> usize {
        lock_or_recover(&self.state).running
    }
}

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = lock_or_recover(&self.gate.state);
        state.running -= 1;
        self.gate.changed.notify_all();
    }
}

fn cores() -> usize {
    std::thread::available_parallelism().map_or(2, std::num::NonZero::get)
}

/// Waveform and loudness: audio decodes, light on the cores.
pub fn analysis_gate() -> &'static Gate {
    static GATE: OnceLock<Gate> = OnceLock::new();
    GATE.get_or_init(|| Gate::new(cores().clamp(1, 4)))
}

/// Editing proxies: a decode and an encode each, so about two per four cores.
pub fn proxy_gate() -> &'static Gate {
    static GATE: OnceLock<Gate> = OnceLock::new();
    GATE.get_or_init(|| Gate::new((cores() / 2).clamp(1, 3)))
}

#[cfg(test)]
#[path = "import_gate_test.rs"]
mod tests;
