use crate::event::seal::EvalGuard;
use crate::event::seal::QueryGuard;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::time::Instant;

pub trait Handler
where
    Self: Sized,
{
    type Query;

    type Eval;

    fn query_scope(&self) -> QueryGuard<'_, Self, Self::Query> {
        let value = self.begin_query();
        QueryGuard::new(self, Some(value))
    }

    fn begin_query(&self) -> Self::Query;

    fn exit_query(&self, guard: Self::Query);

    fn eval_scope(&self) -> EvalGuard<'_, Self, Self::Eval> {
        let value = self.begin_eval();
        EvalGuard::new(self, Some(value))
    }

    fn begin_eval(&self) -> Self::Eval;

    fn end_eval(&self, guard: Self::Eval);
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
    type Query = Instant;

    type Eval = Instant;

    fn begin_query(&self) -> Self::Query {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        Instant::now()
    }

    fn exit_query(&self, guard: Self::Query) {
        self.query_time.add(guard.elapsed());
    }

    fn begin_eval(&self) -> Self::Eval {
        self.eval_count.fetch_add(1, Ordering::Relaxed);
        Instant::now()
    }

    fn end_eval(&self, guard: Self::Eval) {
        self.eval_time.add(guard.elapsed());
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
    type Query = ();

    type Eval = ();

    fn begin_query(&self) -> Self::Query {}

    fn exit_query(&self, (): Self::Query) {}

    fn begin_eval(&self) -> Self::Eval {}

    fn end_eval(&self, (): Self::Eval) {}
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

mod seal {
    use crate::event;

    const VALUE_ALREADY_TAKEN: &str = "bug: guard value already taken";

    #[derive(Debug)]
    pub struct QueryGuard<'handler, H, G>
    where
        H: event::Handler<Query = G>,
    {
        handler: &'handler H,
        value: Option<G>,
    }

    #[derive(Debug)]
    pub struct EvalGuard<'handler, H, G>
    where
        H: event::Handler<Eval = G>,
    {
        handler: &'handler H,
        value: Option<G>,
    }

    impl<'handler, H, G> QueryGuard<'handler, H, G>
    where
        H: event::Handler<Query = G>,
    {
        pub const fn new(handler: &'handler H, value: Option<G>) -> Self {
            Self { handler, value }
        }
    }

    impl<H, G> Drop for QueryGuard<'_, H, G>
    where
        H: event::Handler<Query = G>,
    {
        fn drop(&mut self) {
            self.handler
                .exit_query(self.value.take().expect(VALUE_ALREADY_TAKEN));
        }
    }

    impl<'handler, H, G> EvalGuard<'handler, H, G>
    where
        H: event::Handler<Eval = G>,
    {
        pub const fn new(handler: &'handler H, value: Option<G>) -> Self {
            Self { handler, value }
        }
    }

    impl<H, G> Drop for EvalGuard<'_, H, G>
    where
        H: event::Handler<Eval = G>,
    {
        fn drop(&mut self) {
            self.handler
                .end_eval(self.value.take().expect(VALUE_ALREADY_TAKEN));
        }
    }
}
