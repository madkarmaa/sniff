//! Shares one lookup between concurrent requests for the same app.
//!
//! Requests for an uncached app often arrive together: retries, or several
//! clients asking for a trending app. The first request leads and asks
//! Google; the others follow and wait for its outcome instead of each costing
//! the accounts another round of upstream calls.
//!
//! Only plain data passes between requests, and followers check for it on a
//! timer instead of being woken by the leader. The Workers runtime forbids a
//! request from using another request's I/O, and it cancels a request that
//! waits on nothing of its own as hung.
//!
//! The sharing covers one isolate. Requests that land in different isolates
//! or locations still look up separately.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

/// How long a lookup may run before others stop waiting for it. The runtime
/// can cancel a leader's request without dropping its state, and nobody must
/// wait on such a lookup forever. A lookup normally takes a few seconds.
pub const ABANDON_AFTER_MS: u64 = 30_000;

enum State<T> {
    Running {
        started_at_ms: u64,
    },
    Finished(T),
    /// The leader stopped without an outcome, for example because its client
    /// was over the rate limit.
    Abandoned,
}

/// Where a lookup stands, as a follower sees it.
#[derive(Debug, PartialEq, Eq)]
pub enum Progress<T> {
    Running,
    Finished(T),
    /// The leader gave up or ran out of time; join again to retry.
    Abandoned,
}

type Slot<T> = Rc<RefCell<State<T>>>;
type Slots<T> = Rc<RefCell<HashMap<String, Slot<T>>>>;

fn progress<T: Clone>(state: &State<T>, now_ms: u64) -> Progress<T> {
    match state {
        State::Running { started_at_ms }
            if now_ms.saturating_sub(*started_at_ms) >= ABANDON_AFTER_MS =>
        {
            Progress::Abandoned
        }
        State::Running { .. } => Progress::Running,
        State::Finished(outcome) => Progress::Finished(outcome.clone()),
        State::Abandoned => Progress::Abandoned,
    }
}

/// The lookups in progress, by key. Clones share the same lookups.
pub struct InFlight<T> {
    slots: Slots<T>,
}

impl<T> Default for InFlight<T> {
    fn default() -> Self {
        Self {
            slots: Rc::default(),
        }
    }
}

impl<T> Clone for InFlight<T> {
    fn clone(&self) -> Self {
        Self {
            slots: Rc::clone(&self.slots),
        }
    }
}

/// A request's part in the lookup of a key.
pub enum Turn<T> {
    /// Nobody is looking the key up: do it, then hand the outcome over with
    /// `Leader::finish`.
    Lead(Leader<T>),
    /// Somebody is: check `Follower::progress` until it ends.
    Follow(Follower<T>),
}

impl<T: Clone> InFlight<T> {
    /// Lead the lookup of `key`, or follow the one already in progress. A
    /// lookup past `ABANDON_AFTER_MS` is replaced instead of followed.
    #[must_use]
    pub fn join(&self, key: &str, now_ms: u64) -> Turn<T> {
        let mut slots = self.slots.borrow_mut();
        if let Some(slot) = slots.get(key)
            && matches!(progress(&slot.borrow(), now_ms), Progress::Running)
        {
            return Turn::Follow(Follower {
                slot: Rc::clone(slot),
            });
        }
        let slot = Rc::new(RefCell::new(State::Running {
            started_at_ms: now_ms,
        }));
        slots.insert(key.to_string(), Rc::clone(&slot));
        Turn::Lead(Leader {
            slots: Rc::clone(&self.slots),
            key: key.to_string(),
            slot,
        })
    }
}

/// The request that looks a key up. Dropping it without `finish` tells the
/// followers to retry, so one of them can lead instead.
pub struct Leader<T> {
    slots: Slots<T>,
    key: String,
    slot: Slot<T>,
}

impl<T> Leader<T> {
    /// Hand `outcome` to the followers. Requests that join afterwards start a
    /// new lookup; by then the edge cache usually answers them.
    pub fn finish(self, outcome: T) {
        *self.slot.borrow_mut() = State::Finished(outcome);
    }
}

impl<T> Drop for Leader<T> {
    fn drop(&mut self) {
        let mut slots = self.slots.borrow_mut();
        if slots
            .get(&self.key)
            .is_some_and(|slot| Rc::ptr_eq(slot, &self.slot))
        {
            slots.remove(&self.key);
        }
        drop(slots);
        let mut state = self.slot.borrow_mut();
        if matches!(*state, State::Running { .. }) {
            *state = State::Abandoned;
        }
    }
}

/// A request waiting for another request's lookup.
pub struct Follower<T> {
    slot: Slot<T>,
}

impl<T: Clone> Follower<T> {
    #[must_use]
    pub fn progress(&self, now_ms: u64) -> Progress<T> {
        progress(&self.slot.borrow(), now_ms)
    }
}

#[cfg(test)]
#[path = "tests/in_flight.rs"]
mod tests;
