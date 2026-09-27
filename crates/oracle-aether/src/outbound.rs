//! The per-connection outbound queue — **the piece that makes a slow client harmless.**
//!
//! # Why this exists
//!
//! `aeon/docs/2026-09-09-BUGS-archived.md`, the `## BUG-005` entry, records a frozen repro frame *"lost to
//! an emulator control-socket hang before the sprite table could be dumped"*: a hang in the debug
//! transport destroyed irreplaceable evidence that could not be re-frozen. `protocol.md` §8 item 4 turns
//! that incident into a requirement — event writes must be *"thread-safe … non-blocking on a slow/dead
//! client"*.
//!
//! ⚑ This and three sibling sites cited `aeon/docs/BUGS.md:494-551` until the lens sweep. aeon RETIRED that
//! file at `d7252d8a` (*"eight survivors confirmed and migrated to the ledger, the file archived not
//! deleted"*), so the path resolved to nothing at any aeon revision after 2026-09-09 — worse than a drifted
//! line, because a reader cannot tell a retired file from a mistyped one. Re-anchored on the entry HEADING,
//! which survived the archive; verified at aeon `origin/master` `d3b01f07` through the object store, never
//! through the sibling working tree.
//!
//! # The rule, stated precisely
//!
//! **The emulator thread never writes to a socket and never waits on one.** It calls
//! [`Outbound::push_event`], which is O(1) under a mutex that is *never held across a write*, and which
//! **drops the oldest queued event** rather than waiting when the queue is full. A dropped event is
//! counted and the count is reported on the next message the client does read, so the loss is visible
//! rather than silent.
//!
//! Responses take the other path: [`Outbound::push_response`] *does* wait for space. That is safe
//! because it is called only from the connection's own reader thread — a client that floods requests
//! while refusing to read its own replies stalls nothing but itself.
//!
//! # Eviction touches events only — never a response
//!
//! A queued response is **never evicted**. Discarding one would leave a JSON-RPC request unanswered
//! forever, and would count as a "dropped event" something that was not an event (contract §2.3:
//! `droppedEvents` counts *events discarded*). Until F-EVENT-EVICTS-RESPONSE the queue was one lane and
//! eviction popped its front whatever that entry was; a response queued ahead of an event flood was lost.
//!
//! So the queue is two lanes — events and responses — sharing one capacity (their *sum* is bounded, as
//! the single lane was) and one per-connection sequence counter stamped at push time. The writer always
//! takes whichever lane head carries the lower sequence number, so **every line that is not dropped
//! reaches the socket in exactly its push order**, events and responses interleaved as pushed. Eviction
//! pops the front of the *event* lane only: the oldest queued event, in O(1).
//!
//! When the queue is full of responses alone there is no event to evict. The arriving event is then the
//! one discarded (and counted): the engine must not block, a response must not go, and admitting it
//! past capacity would let an engine flood grow the queue without bound while the responses at its
//! head were stuck behind a slow client. That is the only case in which the *newest* event is the one
//! lost rather than the oldest; the count still reports it.
//!
//! ```text
//!   engine thread ── push_event ──▶ [events   ]─┐ merged by
//!        (never blocks, drops oldest EVENT)      ├─ push seq ─▶ writer thread ──▶ socket
//!   reader thread ── push_response ▶ [responses]─┘             (blocks freely, alone)
//!        (waits for space, never evicted)   (events + responses <= capacity)
//! ```

use std::collections::VecDeque;
use std::num::NonZeroUsize;
use std::sync::{Arc, Condvar, Mutex};

/// Default queue depth per connection. Deep enough that an ordinarily-busy client never loses an event,
/// shallow enough that a dead one costs bounded memory.
pub const DEFAULT_CAPACITY: usize = 1024;

#[derive(Default)]
struct Inner {
    /// Queued server-push events, oldest first, each stamped with its push sequence number. The only
    /// lane eviction ever touches.
    events: VecDeque<(u64, String)>,
    /// Queued responses, oldest first, stamped from the same counter. Never evicted.
    responses: VecDeque<(u64, String)>,
    /// Next push sequence number. Both lanes draw from it, so comparing lane heads recovers the exact
    /// push order. A `u64` at one push per nanosecond wraps after ~584 years.
    next_seq: u64,
    /// Events discarded because the client was not draining. Reported to the client (`droppedEvents`)
    /// so a gap in the stream is never silent.
    dropped: u64,
    closed: bool,
}

/// A bounded outbound message queue shared by a connection's reader, writer and the engine. When full,
/// it drops the oldest queued **event**; responses are never dropped (see the module docs).
pub struct Outbound {
    inner: Mutex<Inner>,
    not_empty: Condvar,
    not_full: Condvar,
    capacity: usize,
}

impl Outbound {
    /// A queue that holds at most `capacity` messages.
    ///
    /// **Non-zero by type** (lens M71). A zero-depth queue cannot carry a single reply, and this used to be
    /// a runtime `assert!` — which fired on each *connection's* thread, so a zero in the configuration left
    /// the socket bound and killed every client at its first byte. The value is now refused where it is
    /// configured (`Server::bind`), and this signature cannot be handed one.
    pub fn new(capacity: NonZeroUsize) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            capacity: capacity.get(),
        }
    }

    /// Queue a server-push event. **Never blocks and never fails.** When the queue is full the oldest
    /// queued *event* is discarded and [`take_dropped`](Self::take_dropped) counts it; a queued response
    /// is never touched. If the queue is full of responses alone, this event is the one discarded (and
    /// counted).
    ///
    /// Returns `false` once the connection is closed, so a broadcaster can prune it. A `true` means the
    /// connection is live, not that this particular event will be delivered.
    pub fn push_event(&self, line: String) -> bool {
        let mut inner = self.lock();
        if inner.closed {
            return false;
        }
        // Normally this evicts at most once (the queue never exceeds capacity); a loop keeps the bound
        // self-healing regardless.
        while inner.len() >= self.capacity {
            if inner.events.pop_front().is_none() {
                // Full of responses: nothing evictable. Drop the arrival instead of blocking.
                inner.dropped += 1;
                return true;
            }
            inner.dropped += 1;
        }
        let seq = inner.stamp();
        inner.events.push_back((seq, line));
        drop(inner);
        self.not_empty.notify_one();
        true
    }

    /// Queue a response to a request from this same connection. **Waits** for space if the queue is
    /// full — see the module docs for why that is safe (it can only ever stall the offending client's
    /// own reader thread, never the engine).
    pub fn push_response(&self, line: String) -> bool {
        let mut inner = self.lock();
        while inner.len() >= self.capacity && !inner.closed {
            inner = self
                .not_full
                .wait(inner)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if inner.closed {
            return false;
        }
        let seq = inner.stamp();
        inner.responses.push_back((seq, line));
        drop(inner);
        self.not_empty.notify_one();
        true
    }

    /// Block until a message is available; `None` once the queue is closed and drained. Called only by
    /// the writer thread, which performs the actual (blocking) socket write **after** the lock is
    /// released.
    pub fn pop(&self) -> Option<String> {
        let mut inner = self.lock();
        loop {
            if let Some(line) = inner.pop_in_push_order() {
                drop(inner);
                self.not_full.notify_one();
                return Some(line);
            }
            if inner.closed {
                return None;
            }
            inner = self
                .not_empty
                .wait(inner)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
    }

    /// Read and reset the dropped-event counter. Reported to the client so a gap is never silent.
    pub fn take_dropped(&self) -> u64 {
        let mut inner = self.lock();
        std::mem::take(&mut inner.dropped)
    }

    /// Close the queue: wakes the writer, unblocks any waiting `push_response`, and makes every later
    /// push a no-op.
    pub fn close(&self) {
        let mut inner = self.lock();
        inner.closed = true;
        drop(inner);
        self.not_empty.notify_all();
        self.not_full.notify_all();
    }

    pub fn is_closed(&self) -> bool {
        self.lock().closed
    }

    /// Current depth — diagnostics and tests only.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether nothing is queued — diagnostics and tests only.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A poisoned mutex here means a writer thread panicked mid-pop. The queue's invariants do not
    /// depend on the panicking section (every mutation is a single `push`/`pop`), so recovering is
    /// strictly better than propagating a panic into the emulator thread.
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl Inner {
    /// Total queued across both lanes — the figure capacity bounds.
    fn len(&self) -> usize {
        self.events.len() + self.responses.len()
    }

    fn stamp(&mut self) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        seq
    }

    /// Take whichever lane head was pushed first, so the writer sees exactly the push order of every
    /// line that was not dropped.
    fn pop_in_push_order(&mut self) -> Option<String> {
        let lane = match (self.events.front(), self.responses.front()) {
            (Some((e, _)), Some((r, _))) => {
                if e < r {
                    &mut self.events
                } else {
                    &mut self.responses
                }
            }
            (Some(_), None) => &mut self.events,
            (None, Some(_)) => &mut self.responses,
            (None, None) => return None,
        };
        lane.pop_front().map(|(_, line)| line)
    }
}

/// The registry of connections that opted into events (`clientCapabilities.events:true`) **and** have
/// sent `initialized` — `protocol.md` §3 forbids pushing to a connection before that notification, so
/// registration happens on `initialized` and nowhere earlier.
///
/// Broadcasting is the only operation the emulator thread performs on a connection, and it is
/// non-blocking by construction (see [`Outbound::push_event`]). Closed connections are pruned as they
/// are discovered, so a client that disappears costs one failed push.
#[derive(Clone, Default)]
pub struct Subscribers {
    inner: Arc<Mutex<Vec<Arc<Outbound>>>>,
}

impl Subscribers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&self, queue: Arc<Outbound>) {
        self.lock().push(queue);
    }

    /// Push one already-serialised NDJSON line to every subscriber. Returns how many received it.
    /// **Never blocks on a socket** — the whole point of this type.
    pub fn broadcast(&self, line: &str) -> usize {
        let mut subs = self.lock();
        subs.retain(|q| !q.is_closed());
        let mut sent = 0;
        for q in subs.iter() {
            if q.push_event(line.to_string()) {
                sent += 1;
            }
        }
        sent
    }

    pub fn len(&self) -> usize {
        self.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Arc<Outbound>>> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn cap(n: usize) -> NonZeroUsize {
        NonZeroUsize::new(n).expect("every depth these tests use is non-zero")
    }

    #[test]
    fn push_event_never_blocks_and_drops_the_oldest() {
        let q = Outbound::new(cap(4));
        for i in 0..100 {
            assert!(q.push_event(format!("e{i}")));
        }
        assert_eq!(q.len(), 4, "queue stays bounded");
        assert_eq!(q.take_dropped(), 96, "every discard is counted");
        assert_eq!(q.take_dropped(), 0, "counter resets when read");
        // The survivors are the *newest* — a live client sees current state, not stale history.
        assert_eq!(q.pop().unwrap(), "e96");
    }

    #[test]
    fn a_full_queue_does_not_stall_the_pusher() {
        let q = Outbound::new(cap(2));
        let start = Instant::now();
        for i in 0..200_000 {
            q.push_event(format!("{i}"));
        }
        assert!(
            start.elapsed() < Duration::from_secs(5),
            "200k pushes into a depth-2 queue must not block; took {:?}",
            start.elapsed()
        );
    }

    #[test]
    fn push_response_waits_for_space_then_proceeds() {
        let q = Arc::new(Outbound::new(cap(1)));
        q.push_event("filler".into());
        let q2 = Arc::clone(&q);
        let t = std::thread::spawn(move || q2.push_response("resp".into()));
        // The responder is parked. Draining one slot must release it.
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(q.pop().unwrap(), "filler");
        assert!(t.join().unwrap());
        assert_eq!(q.pop().unwrap(), "resp");
    }

    #[test]
    fn close_wakes_a_waiting_responder_and_a_waiting_writer() {
        let q = Arc::new(Outbound::new(cap(1)));
        q.push_event("filler".into());
        let q2 = Arc::clone(&q);
        let t = std::thread::spawn(move || q2.push_response("resp".into()));
        std::thread::sleep(Duration::from_millis(20));
        q.close();
        assert!(!t.join().unwrap(), "a closed queue refuses the response");
        assert_eq!(
            q.pop(),
            Some("filler".into()),
            "already-queued drains first"
        );
        assert_eq!(q.pop(), None);
        assert!(!q.push_event("late".into()));
    }

    /// Drain everything currently queued without blocking on an empty queue.
    fn drain(q: &Outbound) -> Vec<String> {
        let mut out = Vec::new();
        while !q.is_empty() {
            out.push(q.pop().expect("non-empty queue yields a line"));
        }
        out
    }

    /// F-EVENT-EVICTS-RESPONSE (i). A queued response must survive an event flood of 10x capacity and be
    /// delivered, and `take_dropped` must count exactly the events that were never delivered -- nothing
    /// else. Before the fix, eviction popped the queue front whatever its kind, so the response was
    /// discarded (the request was never answered) and counted as a "dropped event".
    #[test]
    fn a_queued_response_survives_an_event_flood() {
        const CAP: usize = 4;
        let q = Outbound::new(cap(CAP));
        assert!(q.push_response("R".into()));
        let events = 10 * CAP;
        for i in 0..events {
            assert!(
                q.push_event(format!("e{i}")),
                "push_event never fails on an open queue"
            );
        }
        assert_eq!(q.len(), CAP, "the queue stays bounded by its capacity");
        let dropped = q.take_dropped();
        let got = drain(&q);
        assert!(
            got.iter().any(|l| l == "R"),
            "the response was evicted by the event flood: delivered {got:?}"
        );
        let delivered_events = got.iter().filter(|l| l.starts_with('e')).count();
        assert_eq!(
            dropped as usize,
            events - delivered_events,
            "droppedEvents must count only events actually discarded (delivered {got:?})"
        );
        assert_eq!(
            got,
            ["R", "e37", "e38", "e39"],
            "the newest events survive, behind the response"
        );
    }

    /// F-EVENT-EVICTS-RESPONSE (ii). Survivors reach the writer in exactly their push order: a response
    /// still follows the events pushed before it and precedes those pushed after it.
    #[test]
    fn survivors_are_written_in_push_order() {
        let q = Outbound::new(cap(5));
        let pushed = ["e0", "R1", "e1", "R2", "e2", "e3", "e4", "R3-waits", "e5"];
        q.push_event("e0".into());
        q.push_response("R1".into());
        q.push_event("e1".into());
        q.push_response("R2".into());
        q.push_event("e2".into()); // full: [e0 R1 e1 R2 e2]
        q.push_event("e3".into()); // evicts e0
        q.push_event("e4".into()); // evicts e1
        let dropped = q.take_dropped();
        let got = drain(&q);
        assert_eq!(
            got,
            ["R1", "R2", "e2", "e3", "e4"],
            "survivors in push order, both responses kept"
        );
        assert_eq!(dropped, 2, "only e0 and e1 were discarded");
        // Survivors are a subsequence of the push order -- nothing reordered.
        let mut it = pushed.iter();
        for line in &got {
            assert!(
                it.any(|p| p == line),
                "{line} arrived out of push order in {got:?}"
            );
        }

        // Interleaving with events that survive on both sides of a response.
        let q = Outbound::new(cap(4));
        q.push_response("R1".into());
        q.push_event("e0".into());
        q.push_event("e1".into());
        q.push_response("R2".into()); // full: [R1 e0 e1 R2]
        q.push_event("e2".into()); // evicts e0 -- the oldest EVENT, not R1
        assert_eq!(drain(&q), ["R1", "e1", "R2", "e2"]);
        assert_eq!(q.take_dropped(), 1);
    }

    /// F-EVENT-EVICTS-RESPONSE (iii). A queue full of responses only: an arriving event must neither
    /// block nor evict a response. It is the one message that may go, so it is dropped and counted.
    #[test]
    fn an_event_into_a_queue_full_of_responses_is_dropped_not_the_responses() {
        let q = Outbound::new(cap(2));
        assert!(q.push_response("R1".into()));
        assert!(q.push_response("R2".into()));
        let start = Instant::now();
        assert!(
            q.push_event("e0".into()),
            "the queue is open, so the push is accepted (and dropped)"
        );
        assert!(
            start.elapsed() < Duration::from_secs(1),
            "push_event must not block"
        );
        assert_eq!(q.len(), 2, "capacity still bounds the queue");
        assert_eq!(
            q.take_dropped(),
            1,
            "the incoming event is the one discarded, and it is counted"
        );
        assert_eq!(
            drain(&q),
            ["R1", "R2"],
            "both responses delivered, in order"
        );
        // With room again, events flow normally.
        assert!(q.push_event("e1".into()));
        assert_eq!(drain(&q), ["e1"]);
        assert_eq!(q.take_dropped(), 0);
    }
}
