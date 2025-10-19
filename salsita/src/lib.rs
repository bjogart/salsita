extern crate alloc;

use crate::intern::MemoData;
use crate::intern::MemoId;
use crate::metrics::Metrics;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use core::cell::RefCell;
use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;

pub mod intern;
pub mod metrics;
pub mod query;
#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
pub struct Db<M> {
    memos: RefCell<MemoData<M>>,
    rev: GlobalRevision,
    active_queries: RefCell<ActiveQueryStack>,
    metrics: M,
}

#[derive(Debug)]
struct GlobalRevision(AtomicUsize);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
struct Revision(usize);

#[derive(Debug, Default)]
struct ActiveQueryStack {
    ids: Vec<MemoId>,
}

impl<M> Db<M>
where
    M: Metrics,
{
    pub const fn metrics(&self) -> &M {
        &self.metrics
    }

    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let rev = self.rev.get();
        self.memos.borrow_mut().new_input::<I>(rev, value)
    }

    pub fn set_input<I>(&mut self, id: InputId<I>, value: I::Value)
    where
        I: Input,
    {
        let rev = self.rev.incr();
        self.memos
            .borrow_mut()
            .memo_mut(id.memo_id())
            .memoize_at::<I>(rev, value)
    }

    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let query_guard = self.metrics.enter_query();
        let memo = self.memos.borrow_mut().intern_memo::<Q>(args);
        self.resolve_memo(self.rev.get(), memo);
        let out = self.force_memo::<Q>(memo);
        self.metrics.exit_query(query_guard);
        out
    }

    fn resolve_memo(&self, current_rev: Revision, memo_id: MemoId) {
        if let Some(caller) = self.active_queries.borrow().active_query() {
            self.memos.borrow_mut().memo_mut(caller).track_dep(memo_id);
        }
        let (last_verified, deps, has_value) = {
            let last_verified = self.memos.borrow().memo(memo_id).last_verified();
            if last_verified == current_rev {
                return;
            }
            let memos = self.memos.borrow();
            let memo = memos.memo(memo_id);
            let deps = Box::<[MemoId]>::from(memo.deps());
            (last_verified, deps, memo.has_value())
        };
        let deps_postdate_self = deps
            .into_iter()
            .any(|dep| self.verify_dep(current_rev, last_verified, dep));
        if !deps_postdate_self && has_value {
            self.memos
                .borrow_mut()
                .memo_mut(memo_id)
                .verify_at(current_rev);
            return;
        }
        let (eval, args) = {
            let mut memos = self.memos.borrow_mut();
            let entry = memos.memo_mut(memo_id);
            entry.untrack_deps();
            (entry.eval, memos.args(memo_id))
        };
        self.active_queries.borrow_mut().push_query(memo_id);
        let eval_guard = self.metrics.enter_eval();
        let out = eval(self, args.as_ref());
        self.metrics.exit_eval(eval_guard);
        self.active_queries.borrow_mut().pop_query();
        self.memos
            .borrow_mut()
            .memo_mut(memo_id)
            .memoize_at_any(current_rev, out);
    }

    fn verify_dep(&self, current_rev: Revision, memo_last_verified: Revision, dep: MemoId) -> bool {
        self.resolve_memo(current_rev, dep);
        self.memos.borrow().memo(dep).last_verified() > memo_last_verified
    }

    fn force_memo<Q>(&self, memo: MemoId) -> <Q as Query>::Out
    where
        Q: Query,
    {
        self.memos
            .borrow()
            .memo(memo)
            .value()
            .downcast::<Q::Out>()
            .clone()
    }
}

impl GlobalRevision {
    fn incr(&self) -> Revision {
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
        self.ids.last().copied()
    }

    fn push_query(&mut self, id: MemoId) {
        self.ids.push(id);
    }

    fn pop_query(&mut self) {
        self.ids.pop();
    }
}
