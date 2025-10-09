use crate::intern::InputId;
use crate::intern::MemoId;
use crate::intern::RawId;
use crate::metrics::Metrics;
use crate::query::Input;
use crate::query::Query;
use core::any::Any;
use core::any::TypeId;
use core::cell::RefCell;
use std::collections::HashMap;

pub mod intern;
pub mod metrics;
pub mod query;
#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
pub struct Db<M> {
    metrics: M,
    memo_index: MemoIndex,
    memo_entries: MemoEntries,
}

#[derive(Debug, Default)]
struct MemoIndex {
    index: RefCell<HashMap<TypeId, PerQueryIndexAny>>,
}

#[derive(Debug)]
struct PerQueryIndexAny {
    query_index: Box<dyn Any>,
}

struct PerQueryIndex<Q>
where
    Q: Query,
{
    query_index: RefCell<HashMap<Q::Args, MemoId>>,
}

#[derive(Debug, Default)]
struct MemoEntries {
    entries: RefCell<Vec<RefCell<MemoEntry>>>,
}

#[derive(Debug)]
struct MemoEntry {
    state: MemoState,
    value: Option<MemoValueAny>,
}

#[derive(Debug)]
enum MemoState {
    InProgress,
    Ready,
}

#[derive(Debug)]
struct MemoValueAny(Box<dyn Any>);

impl<M> Db<M>
where
    M: Metrics,
{
    pub const fn metrics(&mut self) -> &mut M {
        &mut self.metrics
    }

    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let memo_id = self.new_memo::<I>(InputId::from);
        self.memo_entries.update_memo_value::<I>(memo_id, value);
        InputId::from(memo_id)
    }

    pub fn set_input<I>(&mut self, id: InputId<I>, value: I::Value)
    where
        I: Input,
    {
        self.memo_entries
            .update_memo_value::<I>(id.memo_id(), value);
    }

    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let query_guard = self.metrics.enter_query::<Q>(args);
        let memo_id = self.get_or_alloc_memo::<Q>(args);
        let out = self
            .memo_entries
            .memo_value::<Q>(memo_id)
            .unwrap_or_else(|| self.compute_memo::<Q>(memo_id, args));
        self.metrics.exit_query::<Q>(query_guard, args, &out);
        out
    }

    fn compute_memo<Q>(&self, memo_id: MemoId, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        self.memo_entries
            .update_memo_state(memo_id, MemoState::InProgress);
        let eval_guard = self.metrics.enter_eval::<Q>(args);
        let out = Q::eval(self, args);
        self.metrics.exit_eval::<Q>(eval_guard, args, &out);
        self.memo_entries
            .update_memo_state(memo_id, MemoState::Ready);
        out
    }

    fn get_or_alloc_memo<Q>(&self, args: &Q::Args) -> MemoId
    where
        Q: Query,
    {
        self.memo_index
            .memo::<Q>(args)
            .unwrap_or_else(|| self.new_memo::<Q>(|_| args.clone()))
    }

    fn new_memo<Q>(&self, make_args: impl FnOnce(MemoId) -> Q::Args) -> MemoId
    where
        Q: Query,
    {
        let id = self.memo_entries.alloc_memo();
        self.memo_index.insert_memo::<Q>(make_args(id), id);
        id
    }
}

impl MemoIndex {
    fn insert_memo<Q>(&self, args: Q::Args, id: MemoId)
    where
        Q: Query,
    {
        let Self { index } = self;
        let mut index = index.borrow_mut();
        let query_index_any = index
            .entry(TypeId::of::<Q>())
            .or_insert_with(PerQueryIndexAny::new::<Q>);
        let PerQueryIndex { query_index } = query_index_any.downcast::<Q>();
        query_index.borrow_mut().insert(args, id);
    }

    fn memo<Q>(&self, args: &Q::Args) -> Option<MemoId>
    where
        Q: Query,
    {
        let Self { index } = self;
        let index = index.borrow();
        let query_index_any = index.get(&TypeId::of::<Q>())?;
        let PerQueryIndex { query_index } = query_index_any.downcast::<Q>();
        query_index.borrow().get(args).copied()
    }
}

impl PerQueryIndexAny {
    fn new<Q>() -> Self
    where
        Q: Query,
    {
        Self {
            query_index: Box::new(PerQueryIndex::<Q> {
                query_index: RefCell::default(),
            }),
        }
    }

    fn downcast<Q>(&self) -> &PerQueryIndex<Q>
    where
        Q: Query,
    {
        self.query_index
            .downcast_ref()
            .unwrap_or_else(|| panic!("type cast failed"))
    }
}

impl MemoEntries {
    fn alloc_memo(&self) -> MemoId {
        let mut entries = self.entries.borrow_mut();
        let raw_id = RawId::new(entries.len());
        entries.push(RefCell::new(MemoEntry::new()));
        MemoId::from(raw_id)
    }

    fn update_memo_value<Q>(&self, id: MemoId, value: Q::Out)
    where
        Q: Query,
    {
        if let Some(entry) = self.entries.borrow().get(id.idx()) {
            let mut entry = entry.borrow_mut();
            entry.value = Some(MemoValueAny::new::<Q>(value));
        }
    }

    fn update_memo_state(&self, id: MemoId, state: MemoState) {
        if let Some(entry) = self.entries.borrow().get(id.idx()) {
            let mut entry = entry.borrow_mut();
            entry.state = state
        }
    }

    fn memo_value<Q>(&self, id: MemoId) -> Option<Q::Out>
    where
        Q: Query,
    {
        let entries = self.entries.borrow();
        let entry = entries.get(id.idx())?.borrow();
        entry.panic_on_cycle();
        entry
            .value
            .as_ref()
            .map(MemoValueAny::downcast::<Q>)
            .cloned()
    }
}

impl MemoEntry {
    const fn new() -> Self {
        Self {
            state: MemoState::Ready,
            value: None,
        }
    }

    fn panic_on_cycle(&self) {
        if matches!(self.state, MemoState::InProgress) {
            panic!("cycle detected")
        }
    }
}

impl MemoValueAny {
    fn new<Q>(value: Q::Out) -> Self
    where
        Q: Query,
    {
        Self(Box::new(value))
    }

    fn downcast<Q>(&self) -> &Q::Out
    where
        Q: Query,
    {
        self.0
            .downcast_ref()
            .unwrap_or_else(|| panic!("type cast failed"))
    }
}
