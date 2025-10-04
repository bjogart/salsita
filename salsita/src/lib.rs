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
    state: MemoState,
    value: Option<AnyMemoValue>,
}

enum MemoState {
    InProgress,
    Ready,
}

struct AnyMemoValue(Box<dyn Any>);

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
        let memo_id = Self::new_memo::<I>(store, InputId::from);
        Self::memo_entry(store, memo_id).set_value::<I>(value);
        InputId::from(memo_id)
    }

    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let mut store = self.store.borrow_mut();
        let (memo_id, value) = match Self::query_memos::<Q>(&mut store).0.get(args).copied() {
            None => {
                let memo_id = Self::new_memo::<Q>(&mut store, |_| args.clone());
                (memo_id, None)
            }
            Some(memo_id) => {
                // `MemoEntry::value()` panics if a cycle is detected.
                let value = Self::memo_entry(&mut store, memo_id).value::<Q>();
                (memo_id, value.cloned())
            }
        };
        match value {
            Some(value) => value,
            None => {
                {
                    let mut store = store;
                    Self::memo_entry(&mut store, memo_id).set_state(MemoState::InProgress);
                }
                let out = Q::eval(self, args);
                Self::memo_entry(&mut self.store.borrow_mut(), memo_id).set_state(MemoState::Ready);
                out
            }
        }
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

    fn new_memo<Q>(store: &mut Store, make_args: impl FnOnce(MemoId) -> Q::Args) -> MemoId
    where
        Q: Query,
    {
        let entry = MemoEntry::new();
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
    fn new() -> Self {
        Self {
            state: MemoState::Ready,
            value: None,
        }
    }

    fn set_state(&mut self, state: MemoState) {
        self.state = state;
    }

    fn set_value<S>(&mut self, value: S::Out)
    where
        S: Sig,
    {
        self.value = Some(AnyMemoValue::new::<S>(value));
    }

    fn value<S>(&self) -> Option<&S::Out>
    where
        S: Sig,
    {
        self.panic_if_cycle();
        self.value.as_ref().map(AnyMemoValue::downcast::<S>)
    }

    fn panic_if_cycle(&self) {
        if let MemoState::InProgress = self.state {
            panic!("cycle detected")
        }
    }
}

impl AnyMemoValue {
    fn new<S>(value: S::Out) -> Self
    where
        S: Sig,
    {
        Self(Box::new(value))
    }

    fn downcast<S>(&self) -> &S::Out
    where
        S: Sig,
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
