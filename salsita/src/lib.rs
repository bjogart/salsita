extern crate alloc;

use crate::barrier::ExclusiveBarrier;
use crate::event::Event;
use crate::event::ScopedEvent;
use crate::event::ScopedEventKind;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::query::Input;
use crate::query::InputId;
use crate::query::InputRegistry;
use crate::query::Query;
use crate::query_ops::QueryOpsRegistry;
use crate::storage::DefaultStorage;
use crate::storage::Handle;
use crate::storage::Storage;
use crate::storage::Transfer as _;
use crate::update::PendingCommit;
use crate::update::QueryUpdate;
use alloc::sync::Arc;
use core::any::type_name;
use core::cell::RefCell;
use core::mem;
use core::ops::Deref;
use core::sync::atomic::AtomicBool;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use std::collections::HashSet;

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

#[derive(Debug)]
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

#[derive(Debug)]
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
    handler: H,
}

#[derive(Debug)]
struct GlobalRevision(AtomicUsize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Revision(usize);

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
{
    #[must_use]
    pub fn event_handler(&self) -> &H {
        &self.global.handler
    }

    pub fn new_input<I>(&mut self, value: &I::Value) -> InputId<I>
    where
        H: event::Handler,
        I: Input,
    {
        let rev = self.global.rev.get();
        let query_id = self.global.query_ops.query_id::<I>(&self.global.handler);
        let value_id = self.global.storage.store(&self.global.handler, value);
        self.global.inputs.new_input(|input_id| {
            let dummy_args_id = self.global.storage.store(&self.global.handler, &input_id);
            self.global.memos.new_input(
                &self.global.handler,
                rev,
                query_id,
                dummy_args_id,
                value_id,
            )
        })
    }

    pub fn set_input<I>(&mut self, input_id: InputId<I>, value: &I::Value)
    where
        H: event::Handler,
        I: Input,
    {
        self.global.should_cancel.store(true, Ordering::Release);
        self.barrier.wait_for_exclusive_access(&mut self.global);
        let global = Arc::get_mut(&mut self.global).expect(GLOBAL_NOT_EXCLUSIVE);
        global.should_cancel.store(false, Ordering::Release);

        let memo_id = global.inputs.memo_id(input_id);
        let new_value_id = global.storage.store(&global.handler, value);
        if new_value_id != memo_id.args_id {
            let current_rev = global.rev.bump();
            let mut commit = PendingCommit::new(current_rev, memo_id);
            commit.new_value_id = Some(new_value_id);
            commit.apply(&global.memos);
        }
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

    pub fn gc(&mut self)
    where
        H: event::Handler,
    {
        self.barrier.wait_for_exclusive_access(&mut self.global);
        Arc::get_mut(&mut self.global)
            .expect(GLOBAL_NOT_EXCLUSIVE)
            .gc();
    }

    #[must_use]
    pub(crate) fn should_cancel(&self) -> bool {
        self.global.should_cancel.load(Ordering::Relaxed)
    }
}

impl<S, H> Default for Db<S, H>
where
    S: Storage,
    H: Default,
{
    fn default() -> Self {
        Self {
            global: Arc::default(),
            barrier: ExclusiveBarrier::default(),
        }
    }
}

impl<S, H> Snapshot<S, H>
where
    S: Storage,
{
    pub fn query<Q>(&self, args: &Q::Args) -> <S::Handle as Handle>::TypedHandle<Q::Out>
    where
        H: event::Handler,
        Q: Query,
    {
        let _query_guard = &self
            .global
            .handler
            .scoped_event(ScopedEvent::new(ScopedEventKind::Query));
        let query_id = self.global.query_ops.query_id::<Q>(&self.global.handler);
        let args_id = self.global.storage.store(&self.global.handler, args);
        let memo_id = self
            .global
            .memos
            .memo_id(&self.global.handler, query_id, args_id);
        self.verify_memo(self.global.rev.get(), memo_id);
        self.memoized_value::<Q>(memo_id)
    }

    fn verify_memo(&self, current_rev: Revision, memo_id: MemoId<S>)
    where
        H: event::Handler,
    {
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

    fn eval_memo(&self, current_rev: Revision, memo_id: MemoId<S>)
    where
        H: event::Handler,
    {
        let ops = self
            .global
            .query_ops
            .get(memo_id.query_id)
            .expect(QUERY_NOT_REGISTERED);
        let args = self.global.storage.get(memo_id.args_id);
        let mut query_update = self.install_query();
        let out = {
            let _eval_guard = &self
                .global
                .handler
                .scoped_event(ScopedEvent::new(ScopedEventKind::Eval));
            (ops.eval)(self, args)
        };
        let out = (ops.store_output)(&self.global.storage, &self.global.handler, out.as_ref());
        let mut commit = PendingCommit::new(current_rev, memo_id);
        if self.global.memos.memo(memo_id, |memo| memo.value_id) != Some(out) {
            commit.new_value_id = Some(out);
        }
        query_update.commit = Some(commit);
    }

    fn dep_postdates_rev(
        &self,
        current_rev: Revision,
        memo_last_verified: Revision,
        dep: MemoId<S>,
    ) -> bool
    where
        H: event::Handler,
    {
        self.verify_memo(current_rev, dep);
        self.global
            .memos
            .memo(dep, |memo| memo.last_changed > memo_last_verified)
    }

    fn install_query(&self) -> QueryUpdate<'_, S, H>
    where
        H: event::Handler,
    {
        let mut active_queries = self.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        active_queries.push(ActiveQuery::default());
        QueryUpdate::new(self)
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

impl<S, H> Deref for Snapshot<S, H>
where
    S: Storage,
{
    type Target = Db<S, H>;

    fn deref(&self) -> &Self::Target {
        &self.db
    }
}

impl<S, H> GlobalState<S, H>
where
    S: Storage,
{
    fn gc(&mut self)
    where
        H: event::Handler,
    {
        let Self {
            inputs,
            storage,
            memos,
            query_ops,
            handler,
            rev: _,
            should_cancel: _,
        } = self;
        let input_count = inputs.len();
        let mut storage_transfer = storage.transfer();
        let mut memos_transfer = memos.transfer(input_count);
        let mut input_queries = HashSet::with_capacity(input_count);
        let next_inputs = inputs.transfer(|input| {
            let memo_id = input.memo_id();
            input_queries.insert(memo_id.query_id);
            memos_transfer.retain(memo_id, |value_id| {
                input.transfer(&mut storage_transfer, value_id)
            })
        });
        let next_memos = memos_transfer.into_memos();
        let next_storage = storage_transfer.into_storage(handler);

        let prev_memos = {
            // Replace everything first before dropping collected values, since
            // Drop implementations can panic.
            let _prev_inputs = mem::replace(inputs, next_inputs);
            let _prev_storage = mem::replace(storage, next_storage);
            mem::replace(memos, next_memos)
        };

        handler.event(Event::new(event::EventKind::DeregisterMemos(
            prev_memos.len() - memos.len(),
        )));
        query_ops.retain(handler, |query_id| input_queries.contains(&query_id));
    }
}

impl<S, H> Default for GlobalState<S, H>
where
    S: Storage,
    H: Default,
{
    fn default() -> Self {
        Self {
            rev: GlobalRevision::default(),
            should_cancel: AtomicBool::default(),
            inputs: InputRegistry::default(),
            storage: S::default(),
            memos: Memos::default(),
            query_ops: QueryOpsRegistry::default(),
            handler: H::default(),
        }
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

impl GlobalRevision {
    fn bump(&self) -> Revision {
        Revision(self.0.fetch_add(1, Ordering::AcqRel) + 1)
    }

    fn get(&self) -> Revision {
        Revision(self.0.load(Ordering::Acquire))
    }
}

impl Default for GlobalRevision {
    fn default() -> Self {
        Self(AtomicUsize::new(Revision::NEVER_VERIFIED.0 + 1))
    }
}

impl Revision {
    const NEVER_VERIFIED: Self = Self(0);
}

pub(crate) fn panic_expected_different_type<T>() -> ! {
    panic!(
        "bug (type mismatch): expected `{}` but found different type (possible database mix-up)",
        type_name::<T>()
    )
}
