extern crate alloc;

use crate::barrier::ExclusiveBarrier;
use crate::event::ScopedEvent;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use crate::query_ops::QueryOps;
use crate::query_ops::QueryOpsRegistry;
use crate::storage::DefaultStorage;
use crate::storage::Downcast;
use crate::storage::Storage;
use crate::update::MemoUpdate;
use crate::update::PendingChange;
use crate::update::PendingCommit;
use crate::update::QueryUpdate;
use alloc::sync::Arc;
use core::any::type_name;
use core::cell::RefCell;
use core::num::NonZeroUsize;
use core::ops::Deref;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

mod barrier;
pub mod event;
pub(crate) mod memo;
pub mod query;
mod query_ops;
pub mod storage;
#[cfg(test)]
mod tests;
mod update;

const INCONSISTENT_STATE: &str = "bug: database in inconsistent state due to panic";

#[derive(Debug, Default)]
pub struct Db<S = DefaultStorage, H = ()>
where
    S: Storage,
{
    global: Arc<GlobalState<S, H>>,
    /// This field must drop after `global` to prevent deadlocks; see
    /// [`ExclusiveBarrier`] for information.
    barrier: ExclusiveBarrier,
}

#[derive(Debug)]
pub struct Snapshot<S = DefaultStorage, H = ()>
where
    S: Storage,
{
    db: Db<S, H>,
    active_queries: RefCell<ActiveQueryStack<S>>,
}

#[derive(Debug, Default)]
struct GlobalState<S, H>
where
    S: Storage,
{
    rev: GlobalRevision,
    should_cancel: AtomicBool,
    storage: S,
    memos: Memos<S>,
    query_ops: QueryOpsRegistry<S, H>,
    event_handler: H,
}

#[derive(Debug)]
struct GlobalRevision(AtomicUsize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Revision(NonZeroUsize);

#[derive(Debug)]
struct ActiveQueryStack<S>(Vec<ActiveQuery<S>>)
where
    S: Storage;

#[derive(Debug)]
struct ActiveQuery<S>
where
    S: Storage,
{
    deps: Vec<MemoId<S>>,
}

impl<S, H> Db<S, H>
where
    S: Storage,
    H: event::Handler,
{
    #[must_use]
    pub fn event_handler(&self) -> &H {
        &self.global.event_handler
    }

    pub fn new_input<I>(&mut self, value: &I::Value) -> InputId<I, S>
    where
        I: Input,
    {
        let rev = self.global.rev.get();
        let query_id = self.global.query_ops.query_id::<I>();
        let value_id = self.global.storage.store(value);
        self.global.storage.store_input_id(|args_id| {
            let memo_id = self
                .global
                .memos
                .new_input(rev, query_id, args_id, value_id);
            InputId::from(memo_id)
        })
    }

    pub fn set_input<I>(&mut self, id: InputId<I, S>, value: &I::Value)
    where
        I: Input,
    {
        self.global.should_cancel.store(true, Ordering::Release);
        self.barrier.wait_for_exclusive_access(&mut self.global);
        self.global.should_cancel.store(false, Ordering::Release);

        let current_rev = self.global.rev.bump();
        let value_id = self.global.storage.store(value);
        let mut commit = PendingCommit::new(current_rev, id.memo_id());
        commit.change = Some(PendingChange::new(value_id));
        let _update = MemoUpdate::new(&self.global.memos, commit);
    }

    #[must_use]
    pub fn snapshot(&self) -> Snapshot<S, H> {
        Snapshot {
            db: Self {
                global: Arc::clone(&self.global),
                barrier: self.barrier.clone(),
            },
            active_queries: RefCell::default(),
        }
    }

    #[must_use]
    pub(crate) fn should_cancel(&self) -> bool {
        self.global.should_cancel.load(Ordering::Relaxed)
    }
}

impl<S, H> Snapshot<S, H>
where
    S: Storage,
    H: event::Handler,
{
    pub fn query<Q>(&self, args: &Q::Args) -> <S::Value as Downcast>::Downcast<Q::Out>
    where
        Q: Query<S>,
    {
        let _query_guard = self.global.event_handler.scoped_event(ScopedEvent::Query);
        let query_id = self.global.query_ops.query_id::<Q>();
        let args_id = self.global.storage.store(args);
        let memo_id = self.global.memos.memo_id(query_id, args_id);
        self.verify_memo(self.global.rev.get(), memo_id);
        self.memoized_value::<Q>(memo_id)
    }

    fn verify_memo(&self, current_rev: Revision, memo_id: MemoId<S>) {
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

    fn eval_memo(&self, current_rev: Revision, memo_id: MemoId<S>) {
        let QueryOps { eval, store_output } = self
            .global
            .query_ops
            .get(memo_id.query_id())
            .expect("bug: query not registered");
        let args = self.global.storage.get(memo_id.args());
        let mut query_update = self.install_query(current_rev, memo_id);
        let out = {
            let _eval_guard = self.global.event_handler.scoped_event(ScopedEvent::Eval);
            eval(self, args)
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
        dep: MemoId<S>,
    ) -> bool {
        self.verify_memo(current_rev, dep);
        self.global
            .memos
            .memo(dep, |memo| memo.last_changed > memo_last_verified)
    }

    fn install_query(&self, current_rev: Revision, memo_id: MemoId<S>) -> QueryUpdate<'_, S, H> {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        active_queries.push(ActiveQuery::default());
        QueryUpdate::new(self, PendingCommit::new(current_rev, memo_id))
    }

    fn track_dep(&self, dep: MemoId<S>) {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        let caller = active_queries.last_mut();
        if let Some(ActiveQuery { deps }) = caller {
            deps.push(dep);
        }
    }

    fn memoized_value<Q>(&self, memo_id: MemoId<S>) -> <S::Value as Downcast>::Downcast<Q::Out>
    where
        Q: Query<S>,
    {
        let value_id = self.global.memos.memo(memo_id, |memo| {
            memo.value_id
                .expect("bug: memo entry has no stored value (value not yet computed or memoized)")
        });
        Downcast::downcast::<Q::Out>(self.global.storage.get(value_id))
    }
}

impl<S> Default for ActiveQueryStack<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self(Vec::default())
    }
}

impl<S> Default for ActiveQuery<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self {
            deps: Vec::default(),
        }
    }
}

impl<S, H> Deref for Snapshot<S, H>
where
    S: Storage,
{
    type Target = Db<S, H>;

    fn deref(&self) -> &Self::Target {
        &self.db
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
