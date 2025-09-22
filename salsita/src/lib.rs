use crate::intern::Id;
use crate::intern::Intern as _;
use core::any::Any;
use core::any::TypeId;
use core::cell::RefCell;
use core::cmp;
use core::fmt;
use core::hash;
use core::hash::Hash;
use core::marker::PhantomData;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

mod intern;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Db {
    registry: RefCell<HashMap<TypeId, QueryId>>,
    // `dyn Any` == `Memos<Sig>`
    queries: RefCell<Vec<Box<dyn Any>>>,
}

#[derive(Clone, Copy)]
struct QueryId(Id);

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

pub trait Query: Sig {
    fn eval(db: &Db, args: &Self::Args) -> Self::Out;
}

pub trait Input: 'static {
    type Value: Clone;
}

pub struct InputId<I>
where
    I: Input,
{
    idx: usize,
    marker: PhantomData<I>,
}

pub trait Sig: 'static {
    type Args: Clone + Eq + Hash;
    type Out: Clone;
}

impl Db {
    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let id = self.get_or_assign_id::<Q>();
        self.ensure_memoized::<Q>(id, args);
        self.unwrap_memoized::<Q>(id, args)
    }

    fn unwrap_memoized<Q>(&self, id: QueryId, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let queries = self.queries.borrow();
        let memos: &Memos<Q> = queries.get(id.idx()).unwrap().downcast_ref().unwrap();
        memos.get(args).unwrap().value().clone()
    }

    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let query_id = self.get_or_assign_id::<I>();
        let input_id = InputId::new(self.memos_len::<I>(query_id));
        self.new_memo::<I>(query_id, input_id, Memo::Ready(value));
        input_id
    }

    fn get_or_assign_id<S>(&self) -> QueryId
    where
        S: Sig,
    {
        match self.registry.borrow_mut().entry(TypeId::of::<S>()) {
            Entry::Vacant(entry) => {
                let mut queries = self.queries.borrow_mut();
                let id = queries.intern(Box::new(Memos::<S>::default())).into();
                *entry.insert(id)
            }
            Entry::Occupied(entry) => *entry.get(),
        }
    }

    fn ensure_memoized<Q>(&self, id: QueryId, args: &Q::Args)
    where
        Q: Query,
    {
        if !self.is_memoized::<Q>(id, args) {
            self.new_memo::<Q>(id, args.clone(), Memo::InProgress);
            let out = Q::eval(self, args);
            self.set_memo::<Q>(id, args, Memo::Ready(out));
        }
    }

    fn is_memoized<S>(&self, id: QueryId, args: &S::Args) -> bool
    where
        S: Sig,
    {
        let queries = self.queries.borrow();
        let memos: &Memos<S> = queries.get(id.idx()).unwrap().downcast_ref().unwrap();
        match memos.get(args) {
            Some(entry) => {
                // Call `entry.value()` for its side effect: panicking if
                // evaluation is circular.
                let _ = entry.value();
                true
            }
            None => false,
        }
    }

    fn memos_len<S>(&self, id: QueryId) -> usize
    where
        S: Sig,
    {
        let queries = self.queries.borrow();
        let memos: &Memos<S> = queries.get(id.idx()).unwrap().downcast_ref().unwrap();
        memos.len()
    }

    fn new_memo<S>(&self, id: QueryId, args: S::Args, memo: Memo<S>)
    where
        S: Sig,
    {
        let mut queries = self.queries.borrow_mut();
        let memos: &mut Memos<S> = queries.get_mut(id.idx()).unwrap().downcast_mut().unwrap();
        memos.insert(args, MemoEntry::new(memo));
    }

    fn set_memo<S>(&self, id: QueryId, args: &S::Args, memo: Memo<S>)
    where
        S: Sig,
        S::Args: Eq + Hash,
    {
        let mut queries = self.queries.borrow_mut();
        let memos: &mut Memos<S> = queries.get_mut(id.idx()).unwrap().downcast_mut().unwrap();
        memos.get_mut(args).unwrap().memo = memo;
    }
}

impl<S> MemoEntry<S>
where
    S: Sig,
{
    fn new(memo: Memo<S>) -> Self {
        Self { memo }
    }

    fn value(&self) -> &S::Out {
        match &self.memo {
            Memo::Ready(value) => value,
            Memo::InProgress => panic!("cycle detected"),
        }
    }
}

impl QueryId {
    fn idx(self) -> usize {
        self.0.idx()
    }
}

impl From<Id> for QueryId {
    fn from(id: Id) -> Self {
        Self(id)
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
