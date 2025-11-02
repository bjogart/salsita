extern crate alloc;

use crate::event::ScopedEvent;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use crate::registry::Ops;
use crate::registry::QueryRegistry;
use crate::storage::DefaultStorage;
use crate::storage::DefaultStorageId;
use crate::storage::Storage as _;
use alloc::sync::Arc;
use core::any::type_name;
use core::cell::RefCell;
use core::num::NonZeroUsize;
use core::ops::Deref;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use std::sync::Condvar;
use std::sync::Mutex;

pub mod event;
pub mod memo;
pub mod query;
mod registry;
pub mod storage;
#[cfg(test)]
mod tests;

const INCONSISTENT_STATE: &str = "bug: database in inconsistent state due to panic";

#[derive(Debug, Default)]
pub struct Db<H = ()> {
    global: Arc<GlobalState<H>>,
    /// Coordinates snapshots with `Arc<GlobalState>` as the counter.
    ///
    /// This field must drop after [`GlobalState`] to ensure [`Db::set_input`]
    /// is notified after the reference count is decremented.
    sync: SnapshotSync,
}

#[derive(Debug)]
pub struct Snapshot<H> {
    db: Db<H>,
    active_queries: RefCell<ActiveQueryStack>,
}

#[derive(Debug, Default)]
struct SnapshotSync(Arc<(Mutex<()>, Condvar)>);

#[derive(Debug, Default)]
struct GlobalState<H> {
    rev: GlobalRevision,
    should_cancel: AtomicBool,
    storage: DefaultStorage,
    memos: Memos,
    registry: QueryRegistry<DefaultStorage, H>,
    event_handler: H,
}

#[derive(Debug)]
struct GlobalRevision(AtomicUsize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Revision(NonZeroUsize);

#[derive(Debug, Default)]
struct ActiveQueryStack(Vec<ActiveQuery>);

#[derive(Debug, Default)]
struct ActiveQuery {
    deps: Vec<MemoId>,
}

#[must_use]
struct QueryUpdate<'snap, H>
where
    H: event::Handler,
{
    snapshot: &'snap Snapshot<H>,
    commit: PendingCommit,
}

#[must_use]
struct MemoUpdate<'memos> {
    memos: &'memos Memos,
    commit: PendingCommit,
}

#[must_use]
struct PendingCommit {
    current_rev: Revision,
    memo_id: MemoId,
    change: Option<PendingChange>,
}

#[must_use]
struct PendingChange {
    value_id: DefaultStorageId,
    deps: Option<Vec<MemoId>>,
}

impl<H> Db<H>
where
    H: event::Handler,
{
    #[must_use]
    pub fn event_handler(&self) -> &H {
        &self.global.event_handler
    }

    pub fn new_input<I>(&mut self, value: &I::Value) -> InputId<I>
    where
        I: Input,
    {
        let rev = self.global.rev.get();
        let query_id = self.global.registry.query_id::<I>();
        let value_id = self.global.storage.store(value);
        self.global.storage.store_input_id(|args_id| {
            let memo_id = self
                .global
                .memos
                .new_input(rev, query_id, args_id, value_id);
            InputId::from(memo_id)
        })
    }

    pub fn set_input<I>(&mut self, id: InputId<I>, value: &I::Value)
    where
        I: Input,
    {
        self.global.should_cancel.store(true, Ordering::Release);
        let global = {
            let SnapshotSync(sync) = &self.sync;
            let (waiter, notifier) = Arc::as_ref(sync);
            let mut guard = waiter.lock().expect(INCONSISTENT_STATE);
            loop {
                if let Some(global) = Arc::get_mut(&mut self.global) {
                    break global;
                }
                guard = notifier.wait(guard).expect(INCONSISTENT_STATE);
            }
        };
        global.should_cancel.store(false, Ordering::Release);

        let current_rev = global.rev.bump();
        let value_id = global.storage.store(value);
        let mut commit = PendingCommit::new(current_rev, id.memo_id());
        commit.change = Some(PendingChange::new(value_id));
        let _update = MemoUpdate::new(&global.memos, commit);
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot<H> {
        Snapshot {
            db: Self {
                global: Arc::clone(&self.global),
                sync: SnapshotSync(Arc::clone(&self.sync.0)),
            },
            active_queries: RefCell::default(),
        }
    }

    #[must_use]
    pub(crate) fn should_cancel(&self) -> bool {
        self.global.should_cancel.load(Ordering::Relaxed)
    }
}

impl<H> Snapshot<H>
where
    H: event::Handler,
{
    pub fn query<Q>(&self, args: &Q::Args) -> Arc<Q::Out>
    where
        Q: Query,
    {
        let _query_guard = self.global.event_handler.scoped_event(ScopedEvent::Query);
        let query_id = self.global.registry.query_id::<Q>();
        let args_id = self.global.storage.store(args);
        let memo_id = self.global.memos.memo_id(query_id, args_id);
        self.verify_memo(self.global.rev.get(), memo_id);
        self.memoized_value::<Q>(memo_id)
    }

    fn verify_memo(&self, current_rev: Revision, memo_id: MemoId) {
        self.track_dep(memo_id);
        let (last_verified, deps) = {
            let (last_verified, deps) = self
                .global
                .memos
                .memo(memo_id, |memo| (memo.last_verified, memo.deps.clone()));
            if last_verified == current_rev
                || self.should_cancel() && last_verified > Revision::NEVER_VERIFIED
            {
                return;
            }
            (last_verified, deps)
        };
        let deps_postdate_memo = deps
            .into_iter()
            .any(|dep| self.dep_postdates_rev(current_rev, last_verified, dep));
        if !deps_postdate_memo && last_verified > Revision::NEVER_VERIFIED {
            self.global
                .memos
                .memo_mut(memo_id, |memo| memo.last_verified = current_rev);
            return;
        }
        self.eval_memo(current_rev, memo_id);
    }

    fn eval_memo(&self, current_rev: Revision, memo_id: MemoId) {
        let Ops {
            eval,
            store_out: store_output,
        } = self
            .global
            .registry
            .get(memo_id.query_id())
            .expect("bug: query not registered");
        let args = self.global.storage.get(memo_id.args());
        let mut query_update = self.install_query(current_rev, memo_id);
        let out = {
            let _eval_guard = self.global.event_handler.scoped_event(ScopedEvent::Eval);
            eval(self, args.as_ref())
        };
        let out = (store_output)(&self.global.storage, out.as_ref());
        if let Some(prev) = self.global.memos.memo(memo_id, |memo| memo.value_id)
            && out == prev
        {
            return;
        }
        query_update.commit.change = Some(PendingChange::new(out));
    }

    fn dep_postdates_rev(
        &self,
        current_rev: Revision,
        memo_last_verified: Revision,
        dep: MemoId,
    ) -> bool {
        self.verify_memo(current_rev, dep);
        self.global
            .memos
            .memo(dep, |memo| memo.last_changed > memo_last_verified)
    }

    fn install_query(&self, current_rev: Revision, memo_id: MemoId) -> QueryUpdate<'_, H> {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        active_queries.push(ActiveQuery::default());
        QueryUpdate::new(self, PendingCommit::new(current_rev, memo_id))
    }

    fn track_dep(&self, dep: MemoId) {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        let caller = active_queries.last_mut();
        if let Some(ActiveQuery { deps }) = caller {
            deps.push(dep);
        }
    }

    fn memoized_value<Q>(&self, memo_id: MemoId) -> Arc<Q::Out>
    where
        Q: Query,
    {
        let value_id = self.global.memos.memo(memo_id, |memo| {
            memo.value_id
                .expect("bug: memo entry has no stored value (value not yet computed or memoized)")
        });
        let Ok(out) = self
            .global
            .storage
            .get(value_id)
            .downcast::<<Q as Query>::Out>()
        else {
            panic_expected_different_type::<<Q as Query>::Out>()
        };
        out
    }
}

impl<'snap, H> QueryUpdate<'snap, H>
where
    H: event::Handler,
{
    const fn new(snapshot: &'snap Snapshot<H>, commit: PendingCommit) -> Self {
        Self { snapshot, commit }
    }
}

impl<H> Drop for QueryUpdate<'_, H>
where
    H: event::Handler,
{
    fn drop(&mut self) {
        let Self { snapshot, commit } = self;
        let mut active_queries = snapshot.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        if let Some(change) = commit.change.as_mut()
            && let Some(ActiveQuery { deps }) = active_queries.pop()
        {
            change.deps = Some(deps);
        }
        let _update = MemoUpdate::new(&snapshot.db.global.memos, commit.take());
    }
}

impl<'memos> MemoUpdate<'memos> {
    const fn new(memos: &'memos Memos, commit: PendingCommit) -> Self {
        Self { memos, commit }
    }
}

impl Drop for MemoUpdate<'_> {
    fn drop(&mut self) {
        let Self {
            memos,
            commit:
                PendingCommit {
                    current_rev,
                    memo_id,
                    change,
                },
        } = self;
        memos.memo_mut(*memo_id, |memo| {
            memo.last_verified = *current_rev;
            if let Some(PendingChange { deps, value_id }) = change.take() {
                memo.value_id = Some(value_id);
                memo.last_changed = *current_rev;
                if let Some(deps) = deps {
                    memo.deps = deps;
                }
            }
        });
    }
}

impl PendingCommit {
    const fn new(current_rev: Revision, memo_id: MemoId) -> Self {
        Self {
            current_rev,
            memo_id,
            change: None,
        }
    }

    const fn take(&mut self) -> Self {
        Self {
            change: self.change.take(),
            current_rev: self.current_rev,
            memo_id: self.memo_id,
        }
    }
}

impl PendingChange {
    const fn new(value_id: DefaultStorageId) -> Self {
        Self {
            value_id,
            deps: None,
        }
    }
}

impl<H> Deref for Snapshot<H> {
    type Target = Db<H>;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

impl Drop for SnapshotSync {
    fn drop(&mut self) {
        let Self(sync) = self;
        let (_, notifier) = Arc::as_ref(sync);
        notifier.notify_all();
    }
}

impl GlobalRevision {
    fn bump(&self) -> Revision {
        self.0.fetch_add(1, Ordering::AcqRel);
        self.get()
    }

    fn get(&self) -> Revision {
        Revision(NonZeroUsize::new(self.0.load(Ordering::Acquire)).expect("revision overflow"))
    }
}

impl Default for GlobalRevision {
    fn default() -> Self {
        Self(AtomicUsize::new(Revision::NEVER_VERIFIED.0.get() + 1))
    }
}

impl Revision {
    const NEVER_VERIFIED: Self = Self(NonZeroUsize::new(1).unwrap());
}

pub(crate) fn panic_expected_different_type<T>() -> ! {
    panic!(
        "bug (type mismatch): expected `{}` but found different type (possible database mix-up)",
        type_name::<T>()
    )
}
