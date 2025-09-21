use core::any::Any;
use core::any::TypeId;
use core::cell::Ref;
use core::cell::RefCell;
use core::cell::RefMut;
use core::cmp;
use core::fmt;
use core::hash;
use core::hash::Hash;
use core::marker::PhantomData;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Db {
    registry: RefCell<HashMap<TypeId, QueryId>>,
    queries: Queries,
}

#[derive(Clone, Copy)]
struct QueryId {
    idx: usize,
}

#[derive(Default)]
struct Queries(RefCell<Vec<QueryData>>);

struct QueryData {
    memos: Box<dyn Any>,
}

enum MemoEntry<S>
where
    S: Sig,
{
    InProgress,
    Ready(S::Output),
}

#[allow(type_alias_bounds)]
type Memos<S>
where
    S: Sig,
= HashMap<S::Args, MemoEntry<S>>;

pub trait Query: Sig {
    fn eval(db: &Db, args: &Self::Args) -> Self::Output;
}

pub trait Input: 'static {
    type Value;
}

pub struct InputId<I>
where
    I: Input,
{
    idx: usize,
    marker: PhantomData<I>,
}

pub trait Sig: 'static {
    type Args;
    type Output;
}

impl Db {
    pub fn query<Q>(&self, args: &Q::Args) -> Q::Output
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
        Q::Output: Clone,
    {
        let id = self.get_or_assign_id::<Q>();
        self.ensure_memoized::<Q>(id, &args);
        self.memoized::<Q>(id, &args)
    }

    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let query_id = self.get_or_assign_id::<I>();
        let mut query = self.queries.query_mut(query_id);
        let memos = query.memos_mut::<I>();
        let input_id = InputId::new(memos.len());
        memos.insert(input_id, MemoEntry::Ready(value));
        input_id
    }

    fn ensure_memoized<Q>(&self, id: QueryId, args: &Q::Args)
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
    {
        let is_memoized = match self.queries.query_mut(id).memos_mut::<Q>().get(args) {
            Some(MemoEntry::InProgress) => panic!("cycle detected"),
            Some(MemoEntry::Ready(_)) => true,
            None => false,
        };
        if !is_memoized {
            self.queries
                .query_mut(id)
                .memos_mut::<Q>()
                .insert(args.clone(), MemoEntry::InProgress);
            let output = Q::eval(self, args);
            self.queries
                .query_mut(id)
                .memos_mut::<Q>()
                .insert(args.clone(), MemoEntry::Ready(output));
        }
    }

    fn memoized<Q>(&self, id: QueryId, args: &Q::Args) -> Q::Output
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
        Q::Output: Clone,
    {
        match self.queries.query(id).memos::<Q>().get(&args) {
            None => panic!("`Db::memoized` called but value is not memoized"),
            Some(MemoEntry::InProgress) => unreachable!(),
            Some(MemoEntry::Ready(output)) => output.clone(),
        }
    }

    fn get_or_assign_id<S>(&self) -> QueryId
    where
        S: Sig,
    {
        match self.registry.borrow_mut().entry(TypeId::of::<S>()) {
            Entry::Vacant(entry) => *entry.insert(self.queries.register::<S>()),
            Entry::Occupied(entry) => *entry.get(),
        }
    }
}

impl Queries {
    fn query(&self, query: QueryId) -> Ref<'_, QueryData> {
        Ref::map(self.0.borrow(), |queries| queries.get(query.idx).unwrap())
    }

    fn query_mut(&self, query: QueryId) -> RefMut<'_, QueryData> {
        RefMut::map(self.0.borrow_mut(), |queries| {
            queries.get_mut(query.idx).unwrap()
        })
    }

    fn register<S>(&self) -> QueryId
    where
        S: Sig,
    {
        let mut this = self.0.borrow_mut();
        let id = QueryId { idx: this.len() };
        this.push(QueryData::new::<S>());
        id
    }
}

impl QueryData {
    fn new<S>() -> Self
    where
        S: Sig,
    {
        Self {
            memos: Box::new(Memos::<S>::default()),
        }
    }

    fn memos<S>(&self) -> &Memos<S>
    where
        S: Sig,
    {
        self.memos.downcast_ref().unwrap()
    }

    fn memos_mut<S>(&mut self) -> &mut Memos<S>
    where
        S: Sig,
    {
        self.memos.downcast_mut().unwrap()
    }
}

impl<I> Sig for I
where
    I: Input,
{
    type Args = InputId<Self>;
    type Output = <Self as Input>::Value;
}

impl<I> Query for I
where
    I: Input,
{
    fn eval(_: &Db, _: &Self::Args) -> Self::Output {
        unimplemented!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}

impl<I> InputId<I>
where
    I: Input,
{
    fn new(idx: usize) -> Self {
        Self {
            idx,
            marker: PhantomData,
        }
    }
}

impl<I> Clone for InputId<I>
where
    I: Input,
{
    fn clone(&self) -> Self {
        Self {
            idx: self.idx.clone(),
            marker: PhantomData,
        }
    }
}

impl<I> Copy for InputId<I> where I: Input {}

impl<I> PartialEq for InputId<I>
where
    I: Input,
{
    fn eq(&self, other: &Self) -> bool {
        self.idx == other.idx
    }
}

impl<I> Eq for InputId<I> where I: Input {}

impl<I> PartialOrd for InputId<I>
where
    I: Input,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.idx.partial_cmp(&other.idx)
    }
}

impl<I> Ord for InputId<I>
where
    I: Input,
{
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.idx.cmp(&other.idx)
    }
}

impl<I> Hash for InputId<I>
where
    I: Input,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.idx.hash(state);
        self.marker.hash(state);
    }
}

impl<I> fmt::Debug for InputId<I>
where
    I: Input,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputId")
            .field("idx", &self.idx)
            .field("marker", &self.marker)
            .finish()
    }
}
