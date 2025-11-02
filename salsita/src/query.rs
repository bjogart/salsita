use crate::Snapshot;
use crate::event;
use crate::memo::MemoId;
use core::cmp::Ordering;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::hash::Hash;
use core::hash::Hasher;
use core::marker::PhantomData;

pub trait Query: 'static {
    type Args: Clone + Eq + Hash + Send + Sync;
    type Out: Clone + Eq + Hash + Send + Sync;

    fn eval<H>(snapshot: &Snapshot<H>, args: &Self::Args) -> Self::Out
    where
        H: event::Handler;
}

pub trait Input: Send + Sync + 'static {
    type Value: Clone + Eq + Hash + Send + Sync;
}

pub struct InputId<I>(MemoId, PhantomData<I>)
where
    I: Input;

impl<I> Query for I
where
    I: Input,
{
    type Args = InputId<Self>;

    type Out = <Self as Input>::Value;

    fn eval<H>(_: &Snapshot<H>, _: &Self::Args) -> Self::Out
    where
        H: event::Handler,
    {
        panic!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}

impl<I> InputId<I>
where
    I: Input,
{
    pub(crate) const fn memo_id(self) -> MemoId {
        self.0
    }
}

impl<I> From<MemoId> for InputId<I>
where
    I: Input,
{
    fn from(id: MemoId) -> Self {
        Self(id, PhantomData)
    }
}

impl<I> Clone for InputId<I>
where
    I: Input,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<I> Copy for InputId<I> where I: Input {}

impl<I> PartialEq for InputId<I>
where
    I: Input,
{
    fn eq(&self, other: &Self) -> bool {
        let Self(self_id, self_marker) = self;
        let Self(other_id, other_marker) = other;
        self_id == other_id && self_marker == other_marker
    }
}

impl<I> Eq for InputId<I> where I: Input {}

impl<I> PartialOrd for InputId<I>
where
    I: Input,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<I> Ord for InputId<I>
where
    I: Input,
{
    fn cmp(&self, other: &Self) -> Ordering {
        let Self(self_id, self_marker) = self;
        let Self(other_id, other_marker) = other;
        self_id.cmp(other_id).then(self_marker.cmp(other_marker))
    }
}

impl<I> Hash for InputId<I>
where
    I: Input,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        let Self(id, marker) = self;
        id.hash(state);
        marker.hash(state);
    }
}

impl<I> Debug for InputId<I>
where
    I: Input,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        #[expect(dead_code)]
        #[derive(Debug)]
        struct DbgInputId(MemoId);

        DbgInputId(self.memo_id()).fmt(f)
    }
}
