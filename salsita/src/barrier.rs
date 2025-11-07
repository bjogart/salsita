use crate::INCONSISTENT_STATE;
use alloc::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;

/// Synchronization primitive that blocks until associated `Arc<T>` is uniquely
/// owned.
///
/// This struct should be a sibling field of the data it protects, and they
/// should be cloned and dropped together. The barrier must drop _after_ the
/// data is dropped to prevent deadlocks. Dropping this struct will notify
/// threads that are currently waiting for unique access.
#[derive(Clone, Debug, Default)]
pub(crate) struct ExclusiveBarrier(Arc<ExclusiveBarrierInner>);

#[derive(Debug, Default)]
struct ExclusiveBarrierInner {
    waiter: Mutex<()>,
    notifier: Condvar,
}

impl ExclusiveBarrier {
    /// Block the current thread until the current thread has exclusive access
    /// to `data`.
    ///
    /// Note that this method requires that the [`ExclusiveBarrier`] and `data`
    /// are cloned and dropped together, and that the barrier is dropped _last_.
    /// See the docs on [`ExclusiveBarrier`] for more information.
    ///
    /// This method makes no guarantees `data` will remain exclusively borrowed
    /// after `wait_for_exclusive_access` returns. It is the responsibility of
    /// the caller to prohibit cloning of `data` by holding `&mut data` while an
    /// exclusive reference is required.
    pub(crate) fn wait_for_exclusive_access<T>(&self, data: &mut Arc<T>) {
        let mut guard = self.0.waiter.lock().expect(INCONSISTENT_STATE);
        loop {
            if let Some(_) = Arc::get_mut(data) {
                break;
            }
            guard = self.0.notifier.wait(guard).expect(INCONSISTENT_STATE);
        }
    }
}

impl Drop for ExclusiveBarrier {
    fn drop(&mut self) {
        self.0.notifier.notify_all();
    }
}
