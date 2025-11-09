extern crate alloc;

use crate::barrier::ExclusiveBarrier;
use crate::event::Event;
use crate::event::ScopedEvent;
use crate::event::ScopedEventKind;
use crate::memo::MemoEntry;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::query::Input;
use crate::query::InputId;
use crate::query::InputRegistry;
use crate::query::Query;
use crate::query_ops::QueryOpsRegistry;
use crate::query_ops::QueryOpsRegistryInner;
use crate::storage::DefaultStorage;
use crate::storage::Handle;
use crate::storage::Storage;
use crate::storage::Transfer as _;
use crate::update::MemoUpdate;
use crate::update::PendingChange;
use crate::update::PendingCommit;
use crate::update::QueryUpdate;
use alloc::sync::Arc;
use core::any::type_name;
use core::cell::RefCell;
use core::mem;
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

pub const INCONSISTENT_STATE: &str = "bug: database in inconsistent state due to panic";
const GLOBAL_NOT_EXCLUSIVE: &str = "bug: `self.global` should be uniquely owned at this point";
const QUERY_NOT_REGISTERED: &str = "bug: query not registered";

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
    inputs: InputRegistry<S>,
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

    pub fn new_input<I>(&mut self, value: &I::Value) -> InputId<I>
    where
        I: Input,
    {
        let rev = self.global.rev.get();
        let query_id = self
            .global
            .query_ops
            .query_id::<I>(&self.global.event_handler);
        let value_id = self.global.storage.store(&self.global.event_handler, value);
        self.global.inputs.new_input(|input_id| {
            let dummy_args_id = self
                .global
                .storage
                .store(&self.global.event_handler, &input_id);
            self.global.memos.new_input(
                &self.global.event_handler,
                rev,
                query_id,
                dummy_args_id,
                value_id,
            )
        })
    }

    pub fn set_input<I>(&mut self, input_id: InputId<I>, value: &I::Value)
    where
        I: Input,
    {
        self.global.should_cancel.store(true, Ordering::Release);
        self.barrier.wait_for_exclusive_access(&mut self.global);
        let global = Arc::get_mut(&mut self.global).expect(GLOBAL_NOT_EXCLUSIVE);
        global.should_cancel.store(false, Ordering::Release);

        let current_rev = global.rev.bump();
        let args_id = global.inputs.memo_id(input_id);
        let mut commit = PendingCommit::new(current_rev, args_id);
        let value_id = global.storage.store(&global.event_handler, value);
        commit.change = Some(PendingChange::new(value_id));
        let _update = MemoUpdate::new(&global.memos, commit);
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

    pub fn gc(&mut self) {
        self.barrier.wait_for_exclusive_access(&mut self.global);
        let global = Arc::get_mut(&mut self.global).expect(GLOBAL_NOT_EXCLUSIVE);

        let mut storage_transfer = mem::take(&mut global.storage).into_transfer();
        let mut query_ops_inner = mem::take(&mut global.query_ops).into_inner();
        let memos = mem::take(&mut global.memos)
            .into_iter()
            .filter_map(|(mut memo_id, mut memo)| {
                // Memos with dependencies are not inputs by definition.
                if !memo.deps.is_empty() {
                    return ignore_memo(&global.event_handler, &mut query_ops_inner, memo_id);
                }
                // Memos without dependencies and without a value are derived
                // queries that have never been evaluated, not inputs.
                let Some(value_id) = memo.value_id else {
                    return ignore_memo(&global.event_handler, &mut query_ops_inner, memo_id);
                };

                let ops = query_ops_inner
                    .get(memo_id.query_id)
                    .expect(QUERY_NOT_REGISTERED);
                let (next_args_id, next_value_id) = (ops.transfer_input_memo_values)(
                    &mut storage_transfer,
                    memo_id.args_id,
                    value_id,
                );
                memo_id.args_id = next_args_id;
                memo.value_id = Some(next_value_id);
                Some((memo_id, memo))
            })
            .collect();

        let _dummy_query_ops = mem::replace(&mut global.query_ops, query_ops_inner.into_registry());
        let _dummy_storage = mem::replace(
            &mut global.storage,
            storage_transfer.into_storage(&global.event_handler),
        );
        let _dummy_memos = mem::replace(&mut global.memos, memos);

        fn ignore_memo<S, H>(
            handler: &H,
            query_ops_inner: &mut QueryOpsRegistryInner<S, H>,
            memo_id: MemoId<S>,
        ) -> Option<(MemoId<S>, MemoEntry<S>)>
        where
            S: Storage,
            H: event::Handler,
        {
            handler.event(Event::new(event::EventKind::DeregisterMemo));
            query_ops_inner.remove(handler, memo_id.query_id);
            None
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
    pub fn query<Q>(&self, args: &Q::Args) -> <S::Handle as Handle>::TypedHandle<Q::Out>
    where
        Q: Query,
    {
        let _query_guard = &self
            .global
            .event_handler
            .scoped_event(ScopedEvent::new(ScopedEventKind::Query));
        let query_id = self
            .global
            .query_ops
            .query_id::<Q>(&self.global.event_handler);
        let args_id = self.global.storage.store(&self.global.event_handler, args);
        let memo_id = self
            .global
            .memos
            .memo_id(&self.global.event_handler, query_id, args_id);
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
        let ops = self
            .global
            .query_ops
            .get(memo_id.query_id)
            .expect(QUERY_NOT_REGISTERED);
        let args = self.global.storage.get(memo_id.args_id);
        let mut query_update = self.install_query(current_rev, memo_id);
        let out = {
            let _eval_guard = &self
                .global
                .event_handler
                .scoped_event(ScopedEvent::new(ScopedEventKind::Eval));
            (ops.eval)(self, args)
        };
        let out = (ops.store_output)(
            &self.global.storage,
            &self.global.event_handler,
            out.as_ref(),
        );
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

    fn memoized_value<Q>(&self, memo_id: MemoId<S>) -> <S::Handle as Handle>::TypedHandle<Q::Out>
    where
        Q: Query,
    {
        let value_id = self.global.memos.memo(memo_id, |memo| {
            memo.value_id
                .expect("bug: memo entry has no stored value (value not yet computed or memoized)")
        });
        Handle::downcast::<Q::Out>(self.global.storage.get(value_id))
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
