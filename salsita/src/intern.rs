#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct MemoId(RawId);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct RawId {
    idx: usize,
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
