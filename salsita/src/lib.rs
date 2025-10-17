use crate::intern::InputId;
use crate::intern::MemoId;
use crate::intern::RawId;
use crate::metrics::Metrics;
use crate::query::Input;
use crate::query::Query;
use core::any::Any;
use core::any::TypeId;
use core::cell::Ref;
use core::cell::RefCell;
use std::collections::HashMap;

pub mod intern;
pub mod metrics;
pub mod query;
#[cfg(test)]
mod tests;

#[derive(Debug, Default)]
pub struct Db<M> {
    memo_index: MemoIndex,
    memo_entries: MemoEntries<M>,
    active_queries: ActiveQueryStack,
    metrics: M,
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
struct MemoEntries<M> {
    entries: RefCell<Vec<RefCell<MemoEntry<M>>>>,
}

#[derive(Debug)]
struct MemoEntry<M> {
    deps: Vec<MemoId>,
    eval: fn(&Db<M>, &dyn Any) -> MemoValueAny,
    value: Option<MemoValueAny>,
}

#[derive(Debug)]
struct MemoValueAny(Box<dyn Any>);

#[derive(Debug, Default)]
struct ActiveQueryStack {
    ids: RefCell<Vec<MemoId>>,
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
        let memo_id = self.new_memo::<I>(InputId::from);
        self.memo_entries
            .entry_mut(memo_id, |entry| entry.set_value::<I>(value));
        InputId::from(memo_id)
    }

    pub fn set_input<I>(&mut self, id: InputId<I>, value: I::Value)
    where
        I: Input,
    {
        self.memo_entries
            .entry_mut(id.memo_id(), |entry| entry.set_value::<I>(value));
    }

    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let query_guard = self.metrics.enter_query();
        let memo_id = self.get_or_alloc_memo::<Q>(args);
        if let Some(caller) = self.active_queries.active_query() {
            self.memo_entries
                .entry_mut(caller, |entry| entry.register_dep(memo_id));
        }
        let has_value = self
            .memo_entries
            .entry(memo_id, |entry| entry.value.is_some());
        let out = if has_value {
            self.memo_entries.entry(memo_id, |entry| {
                entry
                    .value
                    .as_ref()
                    .expect("invariant: memo entry must have a value")
                    .downcast::<Q>()
                    .clone()
            })
        } else {
            let eval = self.memo_entries.entry_mut(memo_id, |entry| {
                entry.deps.clear();
                entry.eval
            });
            self.active_queries.push_query(memo_id);
            let eval_guard = self.metrics.enter_eval();
            let out = eval(self, args).downcast::<Q>().clone();
            self.metrics.exit_eval(eval_guard);
            self.active_queries.pop_query();
            out
        };
        self.metrics.exit_query(query_guard);
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
        let id = self.memo_entries.alloc_entry::<Q>();
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

impl<M> MemoEntries<M>
where
    M: Metrics,
{
    fn alloc_entry<Q>(&self) -> MemoId
    where
        Q: Query,
    {
        let mut entries = self.entries.borrow_mut();
        let raw_id = RawId::new(entries.len());
        entries.push(RefCell::new(MemoEntry::new::<Q>()));
        MemoId::from(raw_id)
    }

    fn entry_mut<T>(&self, id: MemoId, f: impl FnOnce(&mut MemoEntry<M>) -> T) -> T {
        f(&mut self.entry_cell(id).borrow_mut())
    }

    fn entry<T>(&self, id: MemoId, f: impl FnOnce(&MemoEntry<M>) -> T) -> T {
        f(&mut self.entry_cell(id).borrow())
    }

    fn entry_cell(&self, id: MemoId) -> Ref<'_, RefCell<MemoEntry<M>>> {
        Ref::map(self.entries.borrow(), |entries| {
            entries
                .get(id.idx())
                .unwrap_or_else(|| panic!("no entry for ID: {id:?}"))
        })
    }
}

impl<M> MemoEntry<M>
where
    M: Metrics,
{
    const fn new<Q>() -> Self
    where
        Q: Query,
    {
        return Self {
            deps: Vec::new(),
            eval: eval::<M, Q>,
            value: None,
        };

        fn eval<M, Q>(db: &Db<M>, args: &dyn Any) -> MemoValueAny
        where
            M: Metrics,
            Q: Query,
        {
            let args = args
                .downcast_ref()
                .unwrap_or_else(|| panic!("type cast failed"));
            let out = Q::eval(db, args);
            MemoValueAny::new::<Q>(out)
        }
    }

    fn register_dep(&mut self, dep: MemoId) {
        self.deps.push(dep);
    }

    fn set_value<Q>(&mut self, value: <Q>::Out)
    where
        Q: Query,
    {
        self.value = Some(MemoValueAny::new::<Q>(value))
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

impl ActiveQueryStack {
    fn active_query(&self) -> Option<MemoId> {
        self.ids.borrow().last().copied()
    }

    fn push_query(&self, id: MemoId) {
        self.ids.borrow_mut().push(id);
    }

    fn pop_query(&self) {
        self.ids.borrow_mut().pop();
    }
}
