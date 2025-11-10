use crate::INCONSISTENT_STATE;
use crate::Snapshot;
use crate::event;
use crate::memo::MemoId;
use crate::storage::Storage;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::hash::Hash;
use core::hash::Hasher;
use core::marker::PhantomData;
use core::num::NonZeroUsize;
use std::sync::RwLock;

pub trait Query: Send + Sync + 'static {
    type Args: Clone + Eq + Hash + Send + Sync + 'static;
    type Out: Clone + Eq + Hash + Send + Sync + 'static;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler;
}

pub trait Input: Send + Sync + 'static {
    type Value: Clone + Eq + Hash + Send + Sync;
}

#[derive(Debug)]
pub(crate) struct InputRegistry<S>(RwLock<Vec<MemoId<S>>>)
where
    S: Storage;

pub struct InputId<I>(NonZeroUsize, PhantomData<I>);

impl<I> Query for I
where
    I: Input,
{
    type Args = InputId<Self>;

    type Out = <Self as Input>::Value;

    fn eval<S, H>(_: &Snapshot<S, H>, _: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        panic!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}

impl<S> InputRegistry<S>
where
    S: Storage,
{
    pub(crate) fn new_input<I>(&self, f: impl FnOnce(InputId<I>) -> MemoId<S>) -> InputId<I> {
        let mut inputs = self.0.write().expect(INCONSISTENT_STATE);
        let idx = inputs.len();
        let input_id = InputId(
            NonZeroUsize::new(idx + 1).expect("bug: input ID overflow"),
            PhantomData,
        );
        let memo_id = f(input_id);
        inputs.push(memo_id);
        input_id
    }

    pub(crate) fn memo_id<I>(&self, input_id: InputId<I>) -> MemoId<S> {
        let idx = input_id.0.get() - 1;
        *self
            .0
            .read()
            .expect(INCONSISTENT_STATE)
            .get(idx)
            .expect("bug: unknown input ID (was this ID created by another database?)")
    }
}

impl<S> Default for InputRegistry<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self(RwLock::default())
    }
}

impl<I> Clone for InputId<I> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<I> Copy for InputId<I> {}

impl<I> PartialEq for InputId<I> {
    fn eq(&self, other: &Self) -> bool {
        let Self(self_id, self_marker) = self;
        let Self(other_id, other_marker) = other;
        self_id == other_id && self_marker == other_marker
    }
}

impl<I> Eq for InputId<I> {}

impl<I> Hash for InputId<I> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self(id, marker) = self;
        id.hash(state);
        marker.hash(state);
    }
}

impl<I> Debug for InputId<I> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self(memo_id, _marker) = self;
        f.debug_tuple("InputId").field(&memo_id).finish()
    }
}
