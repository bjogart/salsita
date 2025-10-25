extern crate alloc;

use crate::intern::Interner;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::metrics::Metrics;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use core::cell::RefCell;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use std::sync::RwLock;

const INCONSISTENT_STATE: &str = "bug: database in inconsistent state due to panic";

mod intern;
pub mod memo;
pub mod metrics;
pub mod query;
#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
pub struct Db<M = ()> {
    global: GlobalState<M>,
}

#[derive(Debug)]
pub struct Snapshot<M> {
    marker: core::marker::PhantomData<M>,
}

#[derive(Debug, Default)]
struct GlobalState<M> {
    rev: GlobalRevision,
    active_queries: ActiveQueryStack,
    interner: RwLock<Interner>,
    memos: RwLock<Memos<M>>,
    metrics: M,
}

#[derive(Debug)]
struct GlobalRevision(AtomicUsize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Revision(usize);

#[derive(Debug, Default)]
struct ActiveQueryStack {
    ids: RefCell<Vec<MemoId>>,
}

struct PopActiveQuery<'stack> {
    stack: &'stack ActiveQueryStack,
}

impl<M> Db<M>
where
    M: Metrics,
{
    pub const fn metrics(&self) -> &M {
        &self.global.metrics
    }

    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let rev = self.global.rev.get();
        let mut interner = self.global.interner.write().expect(INCONSISTENT_STATE);
        interner.intern_input_id(|args_id| {
            let memo_id = self
                .global
                .memos
                .write()
                .expect(INCONSISTENT_STATE)
                .new_input::<I>(rev, args_id, value);
            InputId::from(memo_id)
        })
    }

    pub fn set_input<I>(&mut self, id: InputId<I>, value: I::Value)
    where
        I: Input,
    {
        let rev = self.global.rev.bump();
        let mut memos = self.global.memos.write().expect(INCONSISTENT_STATE);
        let memo = memos.memo_mut(id.memo_id());
        memo.value = Some(Box::new(value));
        memo.last_verified = rev;
        memo.last_changed = rev;
    }

    pub const fn snapshot(&self) -> Snapshot<M> {
        Snapshot {
            marker: core::marker::PhantomData,
        }
    }

    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let _query_guard = self.global.metrics.query_scope();
        let args_id = self
            .global
            .interner
            .write()
            .expect(INCONSISTENT_STATE)
            .intern(args);
        let memo_id = self
            .global
            .memos
            .write()
            .expect(INCONSISTENT_STATE)
            .intern::<Q>(args_id);
        self.verify_memo(self.global.rev.get(), memo_id);
        self.memoized_value::<Q>(memo_id)
    }

    fn verify_memo(&self, current_rev: Revision, memo_id: MemoId) {
        if let Some(caller) = self.global.active_queries.active_query() {
            self.global
                .memos
                .write()
                .expect(INCONSISTENT_STATE)
                .memo_mut(caller)
                .deps
                .push(memo_id);
        }
        let (last_verified, deps, has_value) = {
            let memos = self.global.memos.read().expect(INCONSISTENT_STATE);
            let memo = memos.memo(memo_id);
            let last_verified = memo.last_verified;
            if last_verified == current_rev {
                return;
            }
            let deps = memo.deps.clone();
            let has_value = memo.value.is_some();
            (last_verified, deps, has_value)
        };
        let deps_postdate_memo = deps
            .into_iter()
            .any(|dep| self.dep_postdates_rev(current_rev, last_verified, dep));
        if !deps_postdate_memo && has_value {
            self.global
                .memos
                .write()
                .expect(INCONSISTENT_STATE)
                .memo_mut(memo_id)
                .last_verified = current_rev;
            return;
        }
        self.eval_memo(current_rev, memo_id);
    }

    fn eval_memo(&self, current_rev: Revision, memo_id: MemoId) {
        let eval = {
            let mut memos = self.global.memos.write().expect(INCONSISTENT_STATE);
            let memo = memos.memo_mut(memo_id);
            memo.deps.clear();
            memo.eval
        };
        let args = self
            .global
            .interner
            .read()
            .expect(INCONSISTENT_STATE)
            .interned(memo_id.args());
        let _stack_len = self.global.active_queries.len();
        let out = {
            let _active_query_guard = self.global.active_queries.push_query(memo_id);
            let _eval_guard = self.global.metrics.eval_scope();
            eval(self, args.as_ref())
        };
        debug_assert_eq!(self.global.active_queries.len(), _stack_len);
        let mut memos = self.global.memos.write().expect(INCONSISTENT_STATE);
        let memo = memos.memo_mut(memo_id);
        memo.last_verified = current_rev;
        if let Some(prev) = memo.value.as_ref()
            && (memo.eq)(out.as_ref(), prev.as_ref())
        {
            return;
        }
        memo.last_changed = current_rev;
        memo.value = Some(out);
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
            .read()
            .expect(INCONSISTENT_STATE)
            .memo(dep)
            .last_changed
            > memo_last_verified
    }

    fn memoized_value<Q>(&self, memo_id: MemoId) -> Q::Out
    where
        Q: Query,
    {
        self.global
            .memos
            .read()
            .expect(INCONSISTENT_STATE)
            .memo(memo_id)
            .value::<Q::Out>()
            .clone()
    }
}

impl GlobalRevision {
    fn bump(&self) -> Revision {
        self.0.fetch_add(1, Ordering::AcqRel);
        self.get()
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

impl ActiveQueryStack {
    fn active_query(&self) -> Option<MemoId> {
        self.ids.borrow().last().copied()
    }

    fn push_query(&self, id: MemoId) -> PopActiveQuery<'_> {
        self.ids.borrow_mut().push(id);
        PopActiveQuery { stack: self }
    }

    fn len(&self) -> usize {
        self.ids.borrow().len()
    }
}

impl Drop for PopActiveQuery<'_> {
    fn drop(&mut self) {
        self.stack.ids.borrow_mut().pop();
    }
}
