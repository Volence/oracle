//! The per-connection outbound queue — **the piece that makes a slow client harmless.**
//!
//! # Why this exists
//!
//! `aeon/docs/BUGS.md:494-551` records a frozen repro frame *"lost to an emulator control-socket hang
//! before the sprite table could be dumped"*: a hang in the debug transport destroyed irreplaceable
//! evidence that could not be re-frozen. `protocol.md` §8 item 4 turns that incident into a requirement —
//! event writes must be *"thread-safe … non-blocking on a slow/dead client"*.
//!
//! # The rule, stated precisely
//!
//! **The emulator thread never writes to a socket and never waits on one.** It calls
//! [`Outbound::push_event`], which is O(1) under a mutex that is *never held across a write*, and which
//! **drops the oldest queued message** rather than waiting when the queue is full. A dropped event is
//! counted and the count is reported on the next message the client does read, so the loss is visible
//! rather than silent.
//!
//! Responses take the other path: [`Outbound::push_response`] *does* wait for space. That is safe
//! because it is called only from the connection's own reader thread — a client that floods requests
//! while refusing to read its own replies stalls nothing but itself.
//!
//! ```text
//!   engine thread ── push_event ──▶ [bounded queue] ──▶ writer thread ──▶ socket
//!        (never blocks, drops oldest)                    (blocks freely, alone)
//! ```

use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

/// Default queue depth per connection. Deep enough that an ordinarily-busy client never loses an event,
/// shallow enough that a dead one costs bounded memory.
pub const DEFAULT_CAPACITY: usize = 1024;

#[derive(Default)]
struct Inner {
    queue: VecDeque<String>,
    /// Events discarded because the client was not draining. Reported to the client (`droppedEvents`)
    /// so a gap in the stream is never silent.
    dropped: u64,
    closed: bool,
}

/// A bounded, drop-oldest outbound message queue shared by a connection's reader, writer and the engine.
pub struct Outbound {
    inner: Mutex<Inner>,
    not_empty: Condvar,
    not_full: Condvar,
    capacity: usize,
}

impl Outbound {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "queue capacity must be positive");
        Self {
            inner: Mutex::new(Inner::default()),
            not_empty: Condvar::new(),
            not_full: Condvar::new(),
            capacity,
        }
    }

    /// Queue a server-push event. **Never blocks and never fails.** When the queue is full the oldest
    /// queued message is discarded and [`take_dropped`](Self::take_dropped) counts it.
    ///
    /// Returns `false` once the connection is closed, so a broadcaster can prune it.
    pub fn push_event(&self, line: String) -> bool {
        let mut inner = self.lock();
        if inner.closed {
            return false;
        }
        while inner.queue.len() >= self.capacity {
            inner.queue.pop_front();
            inner.dropped += 1;
        }
        inner.queue.push_back(line);
        drop(inner);
        self.not_empty.notify_one();
        true
    }

    /// Queue a response to a request from this same connection. **Waits** for space if the queue is
    /// full — see the module docs for why that is safe (it can only ever stall the offending client's
    /// own reader thread, never the engine).
    pub fn push_response(&self, line: String) -> bool {
        let mut inner = self.lock();
        while inner.queue.len() >= self.capacity && !inner.closed {
            inner = self
                .not_full
                .wait(inner)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        }
        if inner.closed {
            return false;
        }
        inner.queue.push_back(line);
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
            if let Some(line) = inner.queue.pop_front() {
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
        self.lock().queue.len()
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

    #[test]
    fn push_event_never_blocks_and_drops_the_oldest() {
        let q = Outbound::new(4);
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
        let q = Outbound::new(2);
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
        let q = Arc::new(Outbound::new(1));
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
        let q = Arc::new(Outbound::new(1));
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
}
