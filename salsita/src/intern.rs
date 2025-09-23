use crate::Input;
use core::cmp;
use core::fmt;
use core::hash;
use core::marker::PhantomData;

pub(crate) trait Intern<T> {
    fn intern(&mut self, v: T) -> Id;
}

pub struct InputId<I>(MemoId, PhantomData<I>)
where
    I: Input;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct MemoId(Id);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct Id {
    idx: usize,
}

impl<T> Intern<T> for Vec<T> {
    fn intern(&mut self, v: T) -> Id {
        let id = Id { idx: self.len() };
        self.push(v);
        id
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
        Self(self.0, PhantomData)
    }
}

impl<I> Copy for InputId<I> where I: Input {}

impl<I> PartialEq for InputId<I>
where
    I: Input,
{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl<I> Eq for InputId<I> where I: Input {}

impl<I> PartialOrd for InputId<I>
where
    I: Input,
{
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        self.0.partial_cmp(&other.0)
    }
}

impl<I> Ord for InputId<I>
where
    I: Input,
{
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl<I> hash::Hash for InputId<I>
where
    I: Input,
{
    fn hash<H: hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
    }
}

impl<I> fmt::Debug for InputId<I>
where
    I: Input,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InputId")
            .field("idx", &self.0)
            .field("marker", &self.1)
            .finish()
    }
}

impl MemoId {
    pub(crate) fn idx(self) -> usize {
        self.0.idx()
    }
}

impl From<Id> for MemoId {
    fn from(id: Id) -> Self {
        Self(id)
    }
}

impl Id {
    pub(crate) fn idx(self) -> usize {
        self.idx
    }
}
