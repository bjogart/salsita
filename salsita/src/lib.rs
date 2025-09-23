use crate::intern::Intern as _;
use crate::intern::QueryId;
use core::any::Any;
use core::any::TypeId;
use core::cell::RefCell;
use core::cmp;
use core::fmt;
use core::hash;
use core::hash::Hash;
use core::marker::PhantomData;
use std::collections::HashMap;

mod intern;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Db {
    store: RefCell<Store>,
}

#[derive(Default)]
struct Store {
    registry: HashMap<TypeId, QueryId>,
    query_memos: Vec<AnyQueryMemos>,
}

struct QueryMemos<S>(HashMap<S::Args, Memo<S>>)
where
    S: Sig;

struct AnyQueryMemos(Box<dyn Any>);

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

pub trait Sig: 'static {
    type Args: Clone + Eq + Hash;
    type Out: Clone;
}

pub struct InputId<I>
where
    I: Input,
{
    idx: usize,
    marker: PhantomData<I>,
}

pub trait Input: 'static {
    type Value: Clone;
}

impl Db {
    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let query_id = self.get_or_assign_id::<I>();
        let input_id = InputId::new(self.memos_len::<I>(query_id));
        self.new_memo::<I>(query_id, input_id, Memo::Ready(value));
        input_id
    }

    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let id = self.get_or_assign_id::<Q>();
        self.ensure_memoized::<Q>(id, args);
        self.unwrap_memoized::<Q>(id, args)
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
        let store = self.store.borrow();
        let memos = store.query_memos.get(id.idx()).unwrap().downcast_ref::<S>();
        match memos.0.get(args) {
            Some(Memo::InProgress) => panic!("cycle detected"),
            Some(Memo::Ready(_)) => true,
            None => false,
        }
    }

    fn unwrap_memoized<Q>(&self, id: QueryId, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let store = self.store.borrow();
        let memos = store.query_memos.get(id.idx()).unwrap().downcast_ref::<Q>();
        match memos.0.get(args).unwrap() {
            Memo::InProgress => panic!("cycle detected"),
            Memo::Ready(value) => value.clone(),
        }
    }

    fn memos_len<S>(&self, id: QueryId) -> usize
    where
        S: Sig,
    {
        let store = self.store.borrow();
        let memos = store.query_memos.get(id.idx()).unwrap().downcast_ref::<S>();
        memos.0.len()
    }

    fn get_or_assign_id<S>(&self) -> QueryId
    where
        S: Sig,
    {
        let key = TypeId::of::<S>();
        let mut store = self.store.borrow_mut();
        match store.registry.get(&key) {
            Some(id) => *id,
            None => {
                let id = QueryId::from(store.query_memos.intern(AnyQueryMemos::new::<S>()));
                store.registry.insert(key, id);
                id
            }
        }
    }

    fn new_memo<S>(&self, id: QueryId, args: S::Args, memo: Memo<S>)
    where
        S: Sig,
    {
        let mut store = self.store.borrow_mut();
        let memos: &mut QueryMemos<S> = store.query_memos.get_mut(id.idx()).unwrap().downcast_mut();
        memos.0.insert(args, memo);
    }

    fn set_memo<S>(&self, id: QueryId, args: &S::Args, memo: Memo<S>)
    where
        S: Sig,
        S::Args: Eq + Hash,
    {
        let mut store = self.store.borrow_mut();
        let memos: &mut QueryMemos<S> = store.query_memos.get_mut(id.idx()).unwrap().downcast_mut();
        *memos.0.get_mut(args).unwrap() = memo;
    }
}

impl AnyQueryMemos {
    fn new<S>() -> Self
    where
        S: Sig,
    {
        Self(Box::new(QueryMemos::<S>(HashMap::default())))
    }

    fn downcast_ref<S>(&self) -> &QueryMemos<S>
    where
        S: Sig,
    {
        match self.0.downcast_ref() {
            Some(this) => this,
            None => panic!("type cast failed"),
        }
    }

    fn downcast_mut<S>(&mut self) -> &mut QueryMemos<S>
    where
        S: Sig,
    {
        match self.0.downcast_mut() {
            Some(this) => this,
            None => panic!("type cast failed"),
        }
    }
}

impl<I> Query for I
where
    I: Input,
{
    fn eval(_: &Db, _: &Self::Args) -> Self::Out {
        unimplemented!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}

impl<I> Sig for I
where
    I: Input,
{
    type Args = InputId<Self>;
    type Out = <Self as Input>::Value;
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
