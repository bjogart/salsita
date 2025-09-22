use core::any::Any;
use core::any::TypeId;
use core::cell::Ref;
use core::cell::RefCell;
use core::cell::RefMut;
use core::cmp;
use core::error::Error;
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
struct Queries(
    // `dyn Any` == `Memos<Sig>`
    RefCell<Vec<Box<dyn Any>>>,
);

#[allow(type_alias_bounds)]
type Memos<S>
where
    S: Sig,
= HashMap<S::Args, MemoEntry<S>>;

struct MemoEntry<S>
where
    S: Sig,
{
    memo: Memo<S>,
}

enum Memo<S>
where
    S: Sig,
{
    InProgress,
    Ready(S::Out),
}

#[derive(Debug)]
struct CycleError;

pub trait Query: Sig {
    fn eval(db: &Db, args: &Self::Args) -> Self::Out;
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
    type Out;
}

impl Db {
    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
        Q::Out: Clone,
    {
        let id = self.get_or_assign_id::<Q>();
        self.ensure_memoized::<Q>(id, args);
        self.queries
            .memos::<Q>(id)
            .get(args)
            .unwrap()
            .memoized_value()
            .unwrap()
            .clone()
    }

    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let query_id = self.get_or_assign_id::<I>();
        let mut memos = self.queries.memos_mut::<I>(query_id);
        let input_id = InputId::new(memos.len());
        memos.insert(input_id, MemoEntry::with_value(value));
        input_id
    }

    fn ensure_memoized<Q>(&self, id: QueryId, args: &Q::Args)
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
    {
        {
            let mut memos = self.queries.memos_mut::<Q>(id);
            let is_memoized = match memos.get(args).map(|entry| entry.memoized_value()) {
                Some(Err(err)) => panic!("{err}"),
                Some(Ok(_)) => true,
                None => false,
            };
            if is_memoized {
                return;
            }
            memos.insert(args.clone(), MemoEntry::in_progress());
        }
        let out = Q::eval(self, args);
        self.queries
            .memos_mut::<Q>(id)
            .insert(args.clone(), MemoEntry::with_value(out));
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
    fn memos<S>(&self, id: QueryId) -> Ref<'_, Memos<S>>
    where
        S: Sig,
    {
        Ref::map(self.0.borrow(), |queries| {
            queries
                .get(id.idx)
                .unwrap()
                .downcast_ref::<Memos<S>>()
                .unwrap()
        })
    }

    fn memos_mut<S>(&self, id: QueryId) -> RefMut<'_, Memos<S>>
    where
        S: Sig,
    {
        RefMut::map(self.0.borrow_mut(), |queries| {
            queries
                .get_mut(id.idx)
                .unwrap()
                .downcast_mut::<Memos<S>>()
                .unwrap()
        })
    }

    fn register<S>(&self) -> QueryId
    where
        S: Sig,
    {
        let mut this = self.0.borrow_mut();
        let id = QueryId { idx: this.len() };
        this.push(Box::new(Memos::<S>::default()));
        id
    }
}

impl<S> MemoEntry<S>
where
    S: Sig,
{
    fn in_progress() -> Self {
        Self {
            memo: Memo::InProgress,
        }
    }

    fn with_value(out: S::Out) -> Self {
        Self {
            memo: Memo::Ready(out),
        }
    }

    fn memoized_value(&self) -> Result<&S::Out, CycleError> {
        match &self.memo {
            Memo::Ready(value) => Ok(value),
            Memo::InProgress => Err(CycleError),
        }
    }
}

impl Error for CycleError {}

impl fmt::Display for CycleError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cycle detected")
    }
}

impl<I> Sig for I
where
    I: Input,
{
    type Args = InputId<Self>;
    type Out = <Self as Input>::Value;
}

impl<I> Query for I
where
    I: Input,
{
    fn eval(_: &Db, _: &Self::Args) -> Self::Out {
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
