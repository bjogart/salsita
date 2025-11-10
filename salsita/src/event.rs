use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::thread;
use std::thread::ThreadId;
use std::time::Instant;

const VALUE_ALREADY_TAKEN: &str = "bug: guard payload already taken";

pub trait Handler
where
    Self: Default + Sized,
{
    type Payload;

    fn event(&self, event: Event);

    fn scoped_event(&self, event: ScopedEvent) -> ScopeGuard<'_, Self> {
        ScopeGuard {
            handler: self,
            payload: Some(self.enter_scope(event)),
        }
    }

    fn enter_scope(&self, event: ScopedEvent) -> Self::Payload;

    fn exit_scope(&self, payload: Self::Payload);
}

#[derive(Clone, Copy, Debug)]
pub struct Event {
    pub thread_id: ThreadId,
    pub kind: EventKind,
}

#[derive(Clone, Copy, Debug)]
pub enum EventKind {
    RegisterQueryOps,
    DeregisterQueryOps,
    StoreValue,
    FreeValues(usize),
    RegisterMemo,
    DeregisterMemo,
}

#[derive(Clone, Copy, Debug)]
pub struct ScopedEvent {
    pub thread_id: ThreadId,
    pub kind: ScopedEventKind,
}

#[derive(Clone, Copy, Debug)]
pub enum ScopedEventKind {
    Query,
    Eval,
}

#[derive(Debug)]
pub struct ScopeGuard<'handler, H>
where
    H: Handler,
{
    handler: &'handler H,
    payload: Option<H::Payload>,
}

#[derive(Debug, Default)]
pub struct PerfHandler {
    query_time: AtomicDuration,
    eval_time: AtomicDuration,
    query_count: AtomicUsize,
    eval_count: AtomicUsize,
    registered_query_ops: AtomicUsize,
    stored_values: AtomicUsize,
    memo_count: AtomicUsize,
}

#[derive(Debug, Default)]
pub struct AtomicDuration {
    ns: AtomicU64,
}

impl Handler for PerfHandler {
    type Payload = (ScopedEventKind, Instant);

    fn event(&self, event: Event) {
        match event.kind {
            EventKind::RegisterQueryOps => {
                self.registered_query_ops.fetch_add(1, Ordering::AcqRel);
            }
            EventKind::DeregisterQueryOps => {
                self.registered_query_ops.fetch_sub(1, Ordering::AcqRel);
            }
            EventKind::StoreValue => {
                self.stored_values.fetch_add(1, Ordering::AcqRel);
            }
            EventKind::FreeValues(count) => {
                self.stored_values.fetch_sub(count, Ordering::AcqRel);
            }
            EventKind::RegisterMemo => {
                self.memo_count.fetch_add(1, Ordering::AcqRel);
            }
            EventKind::DeregisterMemo => {
                self.memo_count.fetch_sub(1, Ordering::AcqRel);
            }
        }
    }

    fn enter_scope(&self, event: ScopedEvent) -> Self::Payload {
        match event.kind {
            ScopedEventKind::Query => self.query_count.fetch_add(1, Ordering::AcqRel),
            ScopedEventKind::Eval => self.eval_count.fetch_add(1, Ordering::AcqRel),
        };
        (event.kind, Instant::now())
    }

    fn exit_scope(&self, payload: Self::Payload) {
        let (kind, start) = payload;
        let elapsed = start.elapsed();
        match kind {
            ScopedEventKind::Query => self.query_time.add(elapsed),
            ScopedEventKind::Eval => self.eval_time.add(elapsed),
        }
    }
}

impl PerfHandler {
    /// Reset metric accumulators.
    ///
    /// This function will reset counts and durations, but not global values,
    /// like number of memos allocated.
    pub fn reset(&self) {
        let Self {
            query_time,
            eval_time,
            query_count,
            eval_count,
            registered_query_ops,
            stored_values,
            memo_count,
        } = self;
        query_time.reset();
        eval_time.reset();
        query_count.store(0, Ordering::Release);
        eval_count.store(0, Ordering::Release);
        registered_query_ops.store(0, Ordering::Release);
        stored_values.store(0, Ordering::Release);
        memo_count.store(0, Ordering::Release);
    }

    pub fn query_time(&self) -> Duration {
        self.query_time.duration()
    }

    pub fn eval_time(&self) -> Duration {
        self.eval_time.duration()
    }

    pub fn query_count(&self) -> usize {
        self.query_count.load(Ordering::Acquire)
    }

    pub fn eval_count(&self) -> usize {
        self.eval_count.load(Ordering::Acquire)
    }

    pub fn registered_query_ops(&self) -> usize {
        self.registered_query_ops.load(Ordering::Acquire)
    }

    pub fn stored_values(&self) -> usize {
        self.stored_values.load(Ordering::Acquire)
    }

    pub fn memo_count(&self) -> usize {
        self.memo_count.load(Ordering::Acquire)
    }
}

impl Handler for () {
    type Payload = ();

    fn event(&self, _: Event) {}

    fn enter_scope(&self, _: ScopedEvent) -> Self::Payload {}

    fn exit_scope(&self, (): Self::Payload) {}
}

impl Event {
    pub(crate) fn new(kind: EventKind) -> Self {
        Self {
            thread_id: thread::current().id(),
            kind,
        }
    }
}

impl ScopedEvent {
    pub(crate) fn new(kind: ScopedEventKind) -> Self {
        Self {
            thread_id: thread::current().id(),
            kind,
        }
    }
}

impl<H> Drop for ScopeGuard<'_, H>
where
    H: Handler,
{
    fn drop(&mut self) {
        self.handler
            .exit_scope(self.payload.take().expect(VALUE_ALREADY_TAKEN));
    }
}

impl AtomicDuration {
    fn add(&self, duration: Duration) {
        let ns = duration.as_nanos().try_into().unwrap_or(u64::MAX);
        self.ns.fetch_add(ns, Ordering::AcqRel);
    }

    fn duration(&self) -> Duration {
        Duration::from_nanos(self.ns.load(Ordering::Acquire))
    }

    fn reset(&self) {
        self.ns.store(0, Ordering::Release);
    }
}
