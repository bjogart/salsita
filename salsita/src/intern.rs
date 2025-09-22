pub(crate) trait Intern<T> {
    fn intern(&mut self, v: T) -> Id;
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct Id {
    idx: usize,
}

#[derive(Clone, Copy)]
pub(crate) struct QueryId(Id);

impl QueryId {
    pub(crate) fn idx(self) -> usize {
        self.0.idx()
    }
}

impl From<Id> for QueryId {
    fn from(id: Id) -> Self {
        Self(id)
    }
}

impl<T> Intern<T> for Vec<T> {
    fn intern(&mut self, v: T) -> Id {
        let id = Id { idx: self.len() };
        self.push(v);
        id
    }
}

impl Id {
    pub(crate) fn idx(self) -> usize {
        self.idx
    }
}
