use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::time::Instant;

const VALUE_ALREADY_TAKEN: &str = "bug: guard payload already taken";

pub trait Handler
where
    Self: Sized,
{
    type Payload;

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
pub enum ScopedEvent {
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
}

#[derive(Debug, Default)]
pub struct AtomicDuration {
    ns: AtomicU64,
}

impl Handler for PerfHandler {
    type Payload = (ScopedEvent, Instant);

    fn enter_scope(&self, event: ScopedEvent) -> Self::Payload {
        match event {
            ScopedEvent::Query => self.query_count.fetch_add(1, Ordering::Relaxed),
            ScopedEvent::Eval => self.eval_count.fetch_add(1, Ordering::Relaxed),
        };
        (event, Instant::now())
    }

    fn exit_scope(&self, payload: Self::Payload) {
        let (event, start) = payload;
        let elapsed = start.elapsed();
        match event {
            ScopedEvent::Query => self.query_time.add(elapsed),
            ScopedEvent::Eval => self.eval_time.add(elapsed),
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
        } = self;
        query_time.reset();
        eval_time.reset();
        query_count.store(0, Ordering::Relaxed);
        eval_count.store(0, Ordering::Relaxed);
    }

    pub fn query_time(&self) -> Duration {
        self.query_time.duration()
    }

    pub fn eval_time(&self) -> Duration {
        self.eval_time.duration()
    }

    pub fn query_count(&self) -> usize {
        self.query_count.load(Ordering::Relaxed)
    }

    pub fn eval_count(&self) -> usize {
        self.eval_count.load(Ordering::Relaxed)
    }
}

impl Handler for () {
    type Payload = ();

    fn enter_scope(&self, _: ScopedEvent) -> Self::Payload {}

    fn exit_scope(&self, (): Self::Payload) {}
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
        self.ns.fetch_add(ns, Ordering::Relaxed);
    }

    fn duration(&self) -> Duration {
        Duration::from_nanos(self.ns.load(Ordering::Relaxed))
    }

    fn reset(&self) {
        self.ns.store(0, Ordering::Relaxed);
    }
}
