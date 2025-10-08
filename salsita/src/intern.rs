use crate::Input;
use core::cmp::Ordering;
use core::fmt;
use core::hash;
use core::marker::PhantomData;

pub struct InputId<I>(MemoId, PhantomData<I>)
where
    I: Input;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct MemoId(RawId);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct RawId {
    idx: usize,
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

impl<I> hash::Hash for InputId<I>
where
    I: Input,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        let Self(id, marker) = self;
        id.hash(state);
        marker.hash(state);
    }
}

impl<I> fmt::Debug for InputId<I>
where
    I: Input,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(id, marker) = self;
        f.debug_struct("InputId")
            .field("id", &id)
            .field("marker", &marker)
            .finish()
    }
}

impl MemoId {
    pub(crate) const fn idx(self) -> usize {
        self.0.idx()
    }
}

impl From<RawId> for MemoId {
    fn from(id: RawId) -> Self {
        Self(id)
    }
}

impl RawId {
    pub(crate) const fn new(idx: usize) -> Self {
        Self { idx }
    }

    pub(crate) const fn idx(self) -> usize {
        self.idx
    }
}
