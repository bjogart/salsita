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
    memo_index: MemoIndex,
    memo_entries: RefCell<Vec<MemoEntry>>,
}

#[derive(Default)]
struct MemoIndex(RefCell<HashMap<TypeId, PerQueryIndexAny>>);

struct PerQueryIndexAny(Box<dyn Any>);

struct PerQueryIndex<Q>(RefCell<HashMap<Q::Args, MemoId>>)
where
    Q: Query;

struct MemoEntry {
    state: MemoState,
    value: Option<AnyMemoValue>,
}

enum MemoState {
    InProgress,
    Ready,
}

struct AnyMemoValue(Box<dyn Any>);

pub trait Query: 'static {
    type Args: Clone + Eq + Hash;
    type Out: Clone;
    fn eval(db: &Db, args: &Self::Args) -> Self::Out;
}

pub trait Input: 'static {
    type Value: Clone;
}

impl Db {
    pub fn new_input<I>(&mut self, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let memo_id =
            Self::new_memo::<I>(&self.memo_index, self.memo_entries.get_mut(), InputId::from);
        Self::memo_entry(self.memo_entries.get_mut(), memo_id).set_value::<I>(value);
        InputId::from(memo_id)
    }

    pub fn set_input<I>(&mut self, id: InputId<I>, value: I::Value)
    where
        I: Input,
    {
        Self::memo_entry(self.memo_entries.get_mut(), id.memo_id()).set_value::<I>(value);
    }

    pub fn query<Q>(&self, args: &Q::Args) -> Q::Out
    where
        Q: Query,
    {
        let mut memo_entries = self.memo_entries.borrow_mut();
        let (memo_id, value) = match self.memo_index.memo::<Q>(args) {
            None => (
                Self::new_memo::<Q>(&self.memo_index, &mut memo_entries, |_| args.clone()),
                None,
            ),
            Some(memo_id) => (
                memo_id,
                Self::memo_entry(&mut memo_entries, memo_id)
                    .value::<Q>()
                    .cloned(),
            ),
        };
        match value {
            Some(value) => value,
            None => {
                {
                    let mut memo_entries = memo_entries;
                    Self::memo_entry(&mut memo_entries, memo_id).set_state(MemoState::InProgress);
                }
                let out = Q::eval(self, args);
                Self::memo_entry(&mut self.memo_entries.borrow_mut(), memo_id)
                    .set_state(MemoState::Ready);
                out
            }
        }
    }

    fn new_memo<Q>(
        memo_index: &MemoIndex,
        memo_entries: &mut Vec<MemoEntry>,
        make_args: impl FnOnce(MemoId) -> Q::Args,
    ) -> MemoId
    where
        Q: Query,
    {
        let entry = MemoEntry::new();
        let raw_id = memo_entries.intern(entry);
        let memo_id = MemoId::from(raw_id);
        memo_index.insert_memo::<Q>(make_args(memo_id), memo_id);
        memo_id
    }

    fn memo_entry(memo_entries: &mut Vec<MemoEntry>, memo_id: MemoId) -> &mut MemoEntry {
        memo_entries.get_mut(memo_id.idx()).unwrap()
    }
}

impl MemoIndex {
    fn insert_memo<Q>(&self, args: Q::Args, id: MemoId)
    where
        Q: Query,
    {
        let mut index = self.0.borrow_mut();
        let query_index_any = index
            .entry(TypeId::of::<Q>())
            .or_insert_with(PerQueryIndexAny::new::<Q>);
        let PerQueryIndex(query_index) = query_index_any.downcast::<Q>();
        query_index.borrow_mut().insert(args, id);
    }

    fn memo<Q>(&self, args: &Q::Args) -> Option<MemoId>
    where
        Q: Query,
    {
        let index = self.0.borrow();
        let query_index_any = index.get(&TypeId::of::<Q>())?;
        let PerQueryIndex(query_index) = query_index_any.downcast::<Q>();
        query_index.borrow().get(args).copied()
    }
}

impl PerQueryIndexAny {
    fn new<Q>() -> Self
    where
        Q: Query,
    {
        Self(Box::new(PerQueryIndex::<Q>(RefCell::default())))
    }

    fn downcast<Q>(&self) -> &PerQueryIndex<Q>
    where
        Q: Query,
    {
        match self.0.downcast_ref() {
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

    fn set_value<Q>(&mut self, value: Q::Out)
    where
        Q: Query,
    {
        self.value = Some(AnyMemoValue::new::<Q>(value));
    }

    fn value<Q>(&self) -> Option<&Q::Out>
    where
        Q: Query,
    {
        self.panic_if_cycle();
        self.value.as_ref().map(AnyMemoValue::downcast::<Q>)
    }

    fn panic_if_cycle(&self) {
        if let MemoState::InProgress = self.state {
            panic!("cycle detected")
        }
    }
}

impl AnyMemoValue {
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

    fn eval(_: &Db, _: &Self::Args) -> Self::Out {
        unimplemented!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}
