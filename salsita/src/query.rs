use crate::Snapshot;
use crate::event;
use crate::memo::MemoId;
use crate::storage::DefaultStorage;
use crate::storage::Storage;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::hash::Hash;
use core::hash::Hasher;
use core::marker::PhantomData;

pub trait Query<S>
where
    Self: 'static,
    S: Storage,
{
    type Args: Clone + Eq + Hash + Send + Sync + 'static;
    type Out: Clone + Eq + Hash + Send + Sync + 'static;

    fn eval<H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        H: event::Handler;
}

pub trait Input: Send + Sync + 'static {
    type Value: Clone + Eq + Hash + Send + Sync;
}

pub struct InputId<I, S = DefaultStorage>(MemoId<S>, PhantomData<I>)
where
    I: Input,
    S: Storage;

impl<I, S> Query<S> for I
where
    I: Input,
    S: Storage,
{
    type Args = InputId<Self, S>;

    type Out = <Self as Input>::Value;

    fn eval<H>(_: &Snapshot<S, H>, _: &Self::Args) -> Self::Out
    where
        H: event::Handler,
    {
        panic!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}

impl<I, S> InputId<I, S>
where
    I: Input,
    S: Storage,
{
    pub(crate) const fn memo_id(self) -> MemoId<S> {
        self.0
    }
}

impl<I, S> From<MemoId<S>> for InputId<I, S>
where
    I: Input,
    S: Storage,
{
    fn from(id: MemoId<S>) -> Self {
        Self(id, PhantomData)
    }
}

impl<I, S> Clone for InputId<I, S>
where
    I: Input,
    S: Storage,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<I, S> Copy for InputId<I, S>
where
    I: Input,
    S: Storage,
{
}

impl<I, S> PartialEq for InputId<I, S>
where
    I: Input,
    S: Storage,
{
    fn eq(&self, other: &Self) -> bool {
        let Self(self_id, self_marker) = self;
        let Self(other_id, other_marker) = other;
        self_id == other_id && self_marker == other_marker
    }
}

impl<I, S> Eq for InputId<I, S>
where
    I: Input,
    S: Storage,
{
}

impl<I, S> Hash for InputId<I, S>
where
    I: Input,
    S: Storage,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self(id, marker) = self;
        id.hash(state);
        marker.hash(state);
    }
}

impl<I, S> Debug for InputId<I, S>
where
    I: Input,
    S: Storage,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self(memo_id, _marker) = self;
        f.debug_tuple("InputId").field(&memo_id).finish()
    }
}
