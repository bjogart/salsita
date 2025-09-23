use crate::intern::InputId;
use crate::intern::Intern as _;
use crate::intern::MemoId;
use core::any::Any;
use core::any::TypeId;
use core::cell::RefCell;
use core::hash::Hash;
use std::collections::HashMap;

pub mod intern;
#[cfg(test)]
mod tests;

#[derive(Default)]
pub struct Db {
    store: RefCell<Store>,
}

#[derive(Default)]
struct Store {
    query_memos: HashMap<QueryId, AnyQueryMemos>,
    memo_entries: Vec<MemoEntry>,
}

#[derive(PartialEq, Eq, Hash)]
struct QueryId(TypeId);

struct AnyQueryMemos(Box<dyn Any>);

struct QueryMemos<S>(HashMap<S::Args, MemoId>)
where
    S: Sig;

struct MemoEntry {
    memo: AnyMemo,
}

struct AnyMemo(Box<dyn Any>);

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

pub trait Input: 'static {
    type Value: Clone;
}

impl Db {
    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let store = self.store.get_mut();
        let memo_id = Self::new_memo::<I>(store, InputId::from, Memo::Ready(value));
        InputId::from(memo_id)
    }

    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let mut store = self.store.borrow_mut();
        let (mut store, memo_id) = match Self::query_memos::<Q>(&mut store).0.get(args).copied() {
            None => {
                let memo_id = {
                    let mut store = store;
                    Self::new_memo::<Q>(&mut store, |_| args.clone(), Memo::InProgress)
                };
                // `Q::eval` might call `Db::query` recursively, and it needs
                // mutable access to `self.store`. The borrow is dropped at the
                // end of the above block, so calling `Q::eval` is safe.
                let out = Q::eval(self, args);
                let mut store = self.store.borrow_mut();
                *Self::memo_entry(&mut store, memo_id)
                    .memo
                    .downcast_mut::<Q>() = Memo::Ready(out);
                (store, memo_id)
            }
            Some(memo_id) => {
                // `Memo::value()` panics if a cycle is detected.
                let _ = Self::memo_entry(&mut store, memo_id)
                    .memo
                    .downcast_mut::<Q>()
                    .value();
                (store, memo_id)
            }
        };
        Self::memo_entry(&mut store, memo_id)
            .memo
            .downcast::<Q>()
            .value()
            .clone()
    }

    fn query_memos<Q>(store: &mut Store) -> &mut QueryMemos<Q>
    where
        Q: Query,
    {
        store
            .query_memos
            .entry(QueryId::new::<Q>())
            .or_insert_with(AnyQueryMemos::new::<Q>)
            .downcast_mut::<Q>()
    }

    fn new_memo<Q>(
        store: &mut Store,
        make_args: impl FnOnce(MemoId) -> Q::Args,
        memo: Memo<Q>,
    ) -> MemoId
    where
        Q: Query,
    {
        let entry = MemoEntry::new(memo);
        let raw_id = store.memo_entries.intern(entry);
        let memo_id = MemoId::from(raw_id);
        Self::query_memos::<Q>(store)
            .0
            .insert(make_args(memo_id), memo_id);
        memo_id
    }

    fn memo_entry(store: &mut Store, memo_id: MemoId) -> &mut MemoEntry {
        store.memo_entries.get_mut(memo_id.idx()).unwrap()
    }
}

impl QueryId {
    fn new<Q>() -> Self
    where
        Q: Query,
    {
        Self(TypeId::of::<Q>())
    }
}

impl AnyQueryMemos {
    fn new<S>() -> Self
    where
        S: Sig,
    {
        Self(Box::new(QueryMemos::<S>(HashMap::default())))
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

impl MemoEntry {
    fn new<S>(memo: Memo<S>) -> Self
    where
        S: Sig,
    {
        Self {
            memo: AnyMemo(Box::new(memo)),
        }
    }
}

impl AnyMemo {
    fn downcast<S>(&self) -> &Memo<S>
    where
        S: Sig,
    {
        match self.0.downcast_ref() {
            Some(this) => this,
            None => panic!("type cast failed"),
        }
    }

    fn downcast_mut<S>(&mut self) -> &mut Memo<S>
    where
        S: Sig,
    {
        match self.0.downcast_mut() {
            Some(this) => this,
            None => panic!("type cast failed"),
        }
    }
}

impl<S> Memo<S>
where
    S: Sig,
{
    fn value(&self) -> &S::Out {
        match self {
            Memo::InProgress => panic!("cycle detected"),
            Memo::Ready(out) => out,
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
