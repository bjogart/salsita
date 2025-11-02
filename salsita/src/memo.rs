use crate::Revision;
use crate::intern::InternId;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::Hash;
use std::collections::HashMap;

const UNKNOWN_ID: &str = "bug: unknown memo ID (was this ID created by another database?)";

#[derive(Debug, Default)]
pub(crate) struct Memos {
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
    pub(crate) cancel_value_id: Option<InternId>,
}

impl Memos {
    pub(crate) fn new_input(
        &mut self,
        rev: Revision,
        query_id: TypeId,
        args_id: InternId,
        value_id: InternId,
    ) -> MemoId {
        let memo_id = MemoId { query_id, args_id };
        let mut entry = MemoEntry::new(None);
        entry.value_id = Some(value_id);
        entry.last_verified = rev;
        entry.last_changed = rev;
        self.memos.insert(memo_id, entry);
        memo_id
    }

    pub(crate) fn memo_id(
        &mut self,
        query_id: TypeId,
        args_id: InternId,
        make_cancel_value_id: impl FnOnce() -> Option<InternId>,
    ) -> MemoId {
        let memo_id = MemoId { query_id, args_id };
        self.memos
            .entry(memo_id)
            .or_insert_with(|| MemoEntry::new(make_cancel_value_id()));
        memo_id
    }

    pub(crate) fn memo_mut(&mut self, id: MemoId) -> &mut MemoEntry {
        self.memos.get_mut(&id).expect(UNKNOWN_ID)
    }

    pub(crate) fn memo(&self, id: MemoId) -> &MemoEntry {
        self.memos.get(&id).expect(UNKNOWN_ID)
    }
}

impl MemoEntry {
    const fn new(cancel_value_id: Option<InternId>) -> Self {
        Self {
            deps: Vec::new(),
            last_verified: Revision::NEVER_VERIFIED,
            last_changed: Revision::NEVER_VERIFIED,
            value_id: None,
            cancel_value_id,
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
