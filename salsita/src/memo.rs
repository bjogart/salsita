use crate::INCONSISTENT_STATE;
use crate::Revision;
use crate::storage::Storage;
use core::any::TypeId;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::hash::Hash;
use core::hash::Hasher;
use std::collections::HashMap;
use std::sync::RwLock;

const UNKNOWN_ID: &str = "bug: unknown memo ID (was this ID created by another database?)";

#[derive(Debug, Default)]
pub(crate) struct Memos<S>(RwLock<HashMap<MemoId<S>, RwLock<MemoEntry<S>>>>)
where
    S: Storage;

pub(crate) struct MemoId<S>
where
    S: Storage,
{
    query_id: TypeId,
    args_id: S::Id,
}

#[derive(Debug)]
pub(crate) struct MemoEntry<S>
where
    S: Storage,
{
    pub(crate) deps: Vec<MemoId<S>>,
    pub(crate) last_verified: Revision,
    pub(crate) last_changed: Revision,
    pub(crate) value_id: Option<S::Id>,
}

impl<S> Memos<S>
where
    S: Storage,
{
    pub(crate) fn new_input(
        &self,
        rev: Revision,
        query_id: TypeId,
        args_id: S::Id,
        value_id: S::Id,
    ) -> MemoId<S> {
        let memo_id = MemoId { query_id, args_id };
        let mut entry = MemoEntry::new();
        entry.value_id = Some(value_id);
        entry.last_verified = rev;
        entry.last_changed = rev;
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .insert(memo_id, RwLock::new(entry));
        memo_id
    }

    pub(crate) fn memo_id(&self, query_id: TypeId, args_id: S::Id) -> MemoId<S> {
        let memo_id = MemoId { query_id, args_id };
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .entry(memo_id)
            .or_insert_with(|| RwLock::new(MemoEntry::new()));
        memo_id
    }

    pub(crate) fn memo_mut(&self, id: MemoId<S>, f: impl FnOnce(&mut MemoEntry<S>)) {
        f(&mut self
            .0
            .read()
            .expect(INCONSISTENT_STATE)
            .get(&id)
            .expect(UNKNOWN_ID)
            .write()
            .expect(INCONSISTENT_STATE))
    }

    pub(crate) fn memo<T>(&self, id: MemoId<S>, f: impl FnOnce(&MemoEntry<S>) -> T) -> T {
        f(&self
            .0
            .read()
            .expect(INCONSISTENT_STATE)
            .get(&id)
            .expect(UNKNOWN_ID)
            .read()
            .expect(INCONSISTENT_STATE))
    }
}

impl<S> MemoEntry<S>
where
    S: Storage,
{
    const fn new() -> Self {
        Self {
            deps: Vec::new(),
            last_verified: Revision::NEVER_VERIFIED,
            last_changed: Revision::NEVER_VERIFIED,
            value_id: None,
        }
    }
}

impl<S> MemoId<S>
where
    S: Storage,
{
    pub(crate) const fn query_id(self) -> TypeId {
        self.query_id
    }

    pub(crate) const fn args(self) -> S::Id {
        self.args_id
    }
}

impl<S> Clone for MemoId<S>
where
    S: Storage,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<S> Copy for MemoId<S> where S: Storage {}

impl<S> PartialEq for MemoId<S>
where
    S: Storage,
{
    fn eq(&self, other: &Self) -> bool {
        self.query_id == other.query_id && self.args_id == other.args_id
    }
}

impl<S> Eq for MemoId<S> where S: Storage {}

impl<S> Hash for MemoId<S>
where
    S: Storage,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.query_id.hash(state);
        self.args_id.hash(state);
    }
}

impl<S> Debug for MemoId<S>
where
    S: Storage,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let Self { query_id, args_id } = self;
        f.debug_struct("MemoId")
            .field("query_id", query_id)
            .field("args_id", args_id)
            .finish()
    }
}
