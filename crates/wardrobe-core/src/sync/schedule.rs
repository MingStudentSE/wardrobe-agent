//! Test-only scheduling gates for deterministic concurrency contracts.
//! Compiled out of production builds; every hook is a no-op unless a unit test
//! installs a gate on its own worker thread, so no cross-test contamination is
//! possible and no business logic is mocked. All waits are bounded so a worker
//! that exits before reaching the hook can never hang the test process.
use std::cell::RefCell;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

const GATE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Default)]
struct GateState {
    parked: bool,
    release: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Gate(Arc<(Mutex<GateState>, Condvar)>);

impl Gate {
    pub(crate) fn new() -> Self {
        Self(Arc::new((Mutex::new(GateState::default()), Condvar::new())))
    }
    fn park(&self) {
        let (state, signal) = &*self.0;
        let mut guard = state.lock().unwrap();
        guard.parked = true;
        signal.notify_all();
        let deadline = Instant::now() + GATE_TIMEOUT;
        while !guard.release {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let (next, _) = signal.wait_timeout(guard, deadline - now).unwrap();
            guard = next;
        }
        // Release or timeout: proceed as a no-op so a lost controller cannot
        // leave the public worker suspended forever.
    }
    /// Returns false when the worker never reached the hook in time.
    pub(crate) fn wait_parked(&self) -> bool {
        let (state, signal) = &*self.0;
        let mut guard = state.lock().unwrap();
        let deadline = Instant::now() + GATE_TIMEOUT;
        while !guard.parked {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (next, _) = signal.wait_timeout(guard, deadline - now).unwrap();
            guard = next;
        }
        true
    }
    pub(crate) fn release(&self) {
        let (state, signal) = &*self.0;
        let mut guard = state.lock().unwrap();
        guard.release = true;
        signal.notify_all();
    }
}

thread_local! {
    static AFTER_PRIOR_READ: RefCell<Option<Gate>> = const { RefCell::new(None) };
}

/// Install on the worker thread before driving the public sync API; the gate
/// only parks that thread's index discovery after its prior cursor read.
pub(crate) fn hold_after_prior_read(gate: Gate) {
    AFTER_PRIOR_READ.with(|slot| *slot.borrow_mut() = Some(gate));
}

pub(crate) fn after_prior_read_point() {
    let gate = AFTER_PRIOR_READ.with(|slot| slot.borrow().clone());
    if let Some(gate) = gate {
        gate.park();
    }
}
