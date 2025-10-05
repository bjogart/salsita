use crate::intern::InputId;
use crate::intern::MemoId;
use crate::intern::RawId;
use crate::metrics::Metrics;
use core::any::Any;
use core::any::TypeId;
use core::cell::RefCell;
use core::hash::Hash;
use std::collections::HashMap;

pub mod intern;
pub mod metrics;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Db<M> {
    metrics: M,
    memo_index: MemoIndex,
    memo_entries: MemoEntries,
}

#[derive(Default)]
struct MemoIndex {
    index: RefCell<HashMap<TypeId, PerQueryIndexAny>>,
}

struct PerQueryIndexAny {
    query_index: Box<dyn Any>,
}

struct PerQueryIndex<Q>
where
    Q: Query,
{
    query_index: RefCell<HashMap<Q::Args, MemoId>>,
}

#[derive(Default)]
struct MemoEntries {
    entries: RefCell<Vec<RefCell<MemoEntry>>>,
}

struct MemoEntry {
    state: MemoState,
    value: Option<MemoValueAny>,
}

enum MemoState {
    InProgress,
    Ready,
}

struct MemoValueAny(Box<dyn Any>);

pub trait Query: 'static {
    type Args: Clone + Eq + Hash;
    type Out: Clone;

    fn eval<M>(db: &Db<M>, args: &Self::Args) -> Self::Out
    where
        M: Metrics;
}

pub trait Input: 'static {
    type Value: Clone;
}

impl<M> Db<M>
where
    M: Metrics,
{
    pub fn metrics(&mut self) -> &mut M {
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
        let out = match self.memo_entries.memo_value::<Q>(memo_id) {
            Some(value) => value,
            None => self.compute_memo::<Q>(memo_id, args),
        };
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
        match self.memo_index.memo::<Q>(args) {
            None => self.new_memo::<Q>(|_| args.clone()),
            Some(memo_id) => memo_id,
        }
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
        match self.query_index.downcast_ref() {
            Some(this) => this,
            None => panic!("type cast failed"),
        }
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
        let entries = self.entries.borrow();
        let mut entry = entries.get(id.idx()).unwrap().borrow_mut();
        entry.value = Some(MemoValueAny::new::<Q>(value));
    }

    fn update_memo_state(&self, id: MemoId, state: MemoState) {
        let entries = self.entries.borrow();
        let mut entry = entries.get(id.idx()).unwrap().borrow_mut();
        entry.state = state
    }

    fn memo_value<Q>(&self, id: MemoId) -> Option<Q::Out>
    where
        Q: Query,
    {
        let entries = self.entries.borrow();
        let entry = entries.get(id.idx()).unwrap().borrow();
        entry.panic_on_cycle();
        entry
            .value
            .as_ref()
            .map(MemoValueAny::downcast::<Q>)
            .cloned()
    }
}

impl MemoEntry {
    fn new() -> Self {
        Self {
            state: MemoState::Ready,
            value: None,
        }
    }

    fn panic_on_cycle(&self) {
        if let MemoState::InProgress = self.state {
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
        match self.0.downcast_ref() {
            Some(this) => this,
            None => panic!("type cast failed"),
        }
    }
}

impl<I> Query for I
where
    I: Input,
{
    type Args = InputId<Self>;

    type Out = <Self as Input>::Value;

    fn eval<M>(_: &Db<M>, _: &Self::Args) -> Self::Out
    where
        M: Metrics,
    {
        unimplemented!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}
