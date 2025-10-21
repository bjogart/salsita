use crate::Query;
use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::time::Instant;

pub trait Metrics {
    type QueryGuard;

    type EvalGuard;

    fn enter_query<Q>(&self, args: &Q::Args) -> Self::QueryGuard
    where
        Q: Query;

    fn exit_query<Q>(&self, guard: Self::QueryGuard, args: &Q::Args, out: &Q::Out)
    where
        Q: Query;

    fn enter_eval<Q>(&self, args: &Q::Args) -> Self::EvalGuard
    where
        Q: Query;

    fn exit_eval<Q>(&self, guard: Self::EvalGuard, args: &Q::Args, out: &Q::Out)
    where
        Q: Query;
}

#[derive(Debug, Default)]
pub struct PerfMetrics {
    query_time: AtomicDuration,
    eval_time: AtomicDuration,
    query_count: AtomicUsize,
    eval_count: AtomicUsize,
}

#[derive(Debug, Default)]
pub struct AtomicDuration {
    ns: AtomicU64,
}

impl Metrics for PerfMetrics {
    type QueryGuard = Instant;

    type EvalGuard = Instant;

    fn enter_query<Q>(&self, _: &Q::Args) -> Self::QueryGuard
    where
        Q: Query,
    {
        self.enter_query_any();
        Instant::now()
    }

    fn exit_query<Q>(&self, guard: Self::QueryGuard, _: &Q::Args, _: &Q::Out)
    where
        Q: Query,
    {
        self.exit_query_any(guard);
    }

    fn enter_eval<Q>(&self, _: &Q::Args) -> Self::EvalGuard
    where
        Q: Query,
    {
        self.enter_eval_any();
        Instant::now()
    }

    fn exit_eval<Q>(&self, guard: Self::EvalGuard, _: &Q::Args, _: &Q::Out)
    where
        Q: Query,
    {
        self.exit_eval_any(guard);
    }
}

impl PerfMetrics {
    /// Reset runtime metrics.
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

    fn enter_query_any(&self) {
        self.query_count.fetch_add(1, Ordering::Relaxed);
    }

    fn exit_query_any(&self, entered_at: Instant) {
        self.query_time.add(entered_at.elapsed());
    }

    fn enter_eval_any(&self) {
        self.eval_count.fetch_add(1, Ordering::Relaxed);
    }

    fn exit_eval_any(&self, entered_at: Instant) {
        self.eval_time.add(entered_at.elapsed());
    }
}

impl Metrics for () {
    type QueryGuard = ();

    type EvalGuard = ();

    fn enter_query<Q>(&self, _: &Q::Args) -> Self::QueryGuard
    where
        Q: Query,
    {
    }

    fn exit_query<Q>(&self, (): Self::QueryGuard, _: &Q::Args, _: &Q::Out)
    where
        Q: Query,
    {
    }

    fn enter_eval<Q>(&self, _: &Q::Args) -> Self::EvalGuard
    where
        Q: Query,
    {
    }

    fn exit_eval<Q>(&self, (): Self::EvalGuard, _: &Q::Args, _: &Q::Out)
    where
        Q: Query,
    {
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
