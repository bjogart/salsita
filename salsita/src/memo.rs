use crate::INCONSISTENT_STATE;
use crate::Revision;
use crate::intern::InternId;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::Hash;
use std::collections::HashMap;
use std::sync::RwLock;

const UNKNOWN_ID: &str = "bug: unknown memo ID (was this ID created by another database?)";

#[derive(Debug, Default)]
pub(crate) struct Memos(RwLock<MemosInner>);

#[derive(Debug, Default)]
struct MemosInner {
    memos: HashMap<MemoId, MemoEntry>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct MemoId {
    query_id: TypeId,
    args_id: InternId,
}

#[derive(Debug)]
pub(crate) struct MemoEntry {
    pub(crate) deps: Vec<MemoId>,
    pub(crate) last_verified: Revision,
    pub(crate) last_changed: Revision,
    pub(crate) value_id: Option<InternId>,
}

impl Memos {
    pub(crate) fn new_input(
        &self,
        rev: Revision,
        query_id: TypeId,
        args_id: InternId,
        value_id: InternId,
    ) -> MemoId {
        let memo_id = MemoId { query_id, args_id };
        let mut entry = MemoEntry::new();
        entry.value_id = Some(value_id);
        entry.last_verified = rev;
        entry.last_changed = rev;
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .memos
            .insert(memo_id, entry);
        memo_id
    }

    pub(crate) fn memo_id(&self, query_id: TypeId, args_id: InternId) -> MemoId {
        let memo_id = MemoId { query_id, args_id };
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .memos
            .entry(memo_id)
            .or_insert_with(MemoEntry::new);
        memo_id
    }

    pub(crate) fn memo_mut(&self, id: MemoId, f: impl FnOnce(&mut MemoEntry)) {
        f(self
            .0
            .write()
            .expect(INCONSISTENT_STATE)
            .memos
            .get_mut(&id)
            .expect(UNKNOWN_ID))
    }

    pub(crate) fn memo<T>(&self, id: MemoId, f: impl FnOnce(&MemoEntry) -> T) -> T {
        f(self
            .0
            .read()
            .expect(INCONSISTENT_STATE)
            .memos
            .get(&id)
            .expect(UNKNOWN_ID))
    }
}

impl MemoEntry {
    const fn new() -> Self {
        Self {
            deps: Vec::new(),
            last_verified: Revision::NEVER_VERIFIED,
            last_changed: Revision::NEVER_VERIFIED,
            value_id: None,
        }
    }
}

impl MemoId {
    pub(crate) const fn query_id(self) -> TypeId {
        self.query_id
    }

    pub(crate) const fn args(self) -> InternId {
        self.args_id
    }
}
