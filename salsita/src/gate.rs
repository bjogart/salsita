use crate::INCONSISTENT_STATE;
use alloc::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;

/// Synchronization primitive that blocks until associated `Arc<T>` is uniquely
/// owned.
///
/// This struct should be a sibling field of the data it protects, and they
/// should be cloned and dropped together. The gate must drop _after_ the  data
/// is dropped to prevent deadlocks. Dropping this struct will notify threads
/// that are currently waiting for unique access.
#[derive(Clone, Debug, Default)]
pub(crate) struct WriteGate(Arc<WriteGateInner>);

#[derive(Debug, Default)]
struct WriteGateInner {
    waiter: Mutex<()>,
    notifier: Condvar,
}

impl WriteGate {
    pub(crate) fn wait_for_write_access<T>(&self, data: &mut Arc<T>) {
        let mut guard = self.0.waiter.lock().expect(INCONSISTENT_STATE);
        loop {
            if let Some(_) = Arc::get_mut(data) {
                break;
            }
            guard = self.0.notifier.wait(guard).expect(INCONSISTENT_STATE);
        }
    }
}

impl Drop for WriteGate {
    fn drop(&mut self) {
        self.0.notifier.notify_all();
    }
}
