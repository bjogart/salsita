use core::sync::atomic::AtomicU64;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use core::time::Duration;
use std::time::Instant;

pub trait Metrics {
    type QueryGuard;

    type EvalGuard;

    fn enter_query(&self) -> Self::QueryGuard;

    fn exit_query(&self, guard: Self::QueryGuard);

    fn enter_eval(&self) -> Self::EvalGuard;

    fn exit_eval(&self, guard: Self::EvalGuard);
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

    fn enter_query(&self) -> Self::QueryGuard {
        self.query_count.fetch_add(1, Ordering::Relaxed);
        Instant::now()
    }

    fn exit_query(&self, guard: Self::QueryGuard) {
        self.query_time.add(guard.elapsed());
    }

    fn enter_eval(&self) -> Self::EvalGuard {
        self.eval_count.fetch_add(1, Ordering::Relaxed);
        Instant::now()
    }

    fn exit_eval(&self, guard: Self::EvalGuard) {
        self.eval_time.add(guard.elapsed());
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
}

impl Metrics for () {
    type QueryGuard = ();

    type EvalGuard = ();

    fn enter_query(&self) -> Self::QueryGuard {}

    fn exit_query(&self, (): Self::QueryGuard) {}

    fn enter_eval(&self) -> Self::EvalGuard {}

    fn exit_eval(&self, (): Self::EvalGuard) {}
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
