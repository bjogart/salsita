use crate::INCONSISTENT_STATE;
use crate::Revision;
use crate::event;
use crate::event::Event;
use crate::event::EventKind;
use crate::query_ops::QueryId;
use crate::storage::Storage;
use core::fmt;
use core::fmt::Debug;
use core::fmt::Formatter;
use core::hash::Hash;
use core::hash::Hasher;
use core::mem;
use std::collections::HashMap;
use std::sync::RwLock;

const UNKNOWN_ID: &str = "bug: unknown memo ID (was this ID created by another database?)";

#[derive(Debug)]
pub(crate) struct Memos<S>(RwLock<MemoMap<S>>)
where
    S: Storage;

#[must_use]
#[derive(Debug)]
pub(crate) struct MemosTransfer<'memos, S>
where
    S: Storage,
{
    prev: MemoMap<S>,
    next: &'memos mut MemoMap<S>,
}

type MemoMap<S> = HashMap<MemoId<S>, RwLock<MemoEntry<S>>>;

pub(crate) struct MemoId<S>
where
    S: Storage,
{
    pub(crate) query_id: QueryId,
    pub(crate) args_id: S::Id,
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
    pub(crate) fn new_input<H>(
        &self,
        handler: &H,
        rev: Revision,
        query_id: QueryId,
        args_id: S::Id,
        value_id: S::Id,
    ) -> MemoId<S>
    where
        H: event::Handler,
    {
        let memo_id = MemoId { query_id, args_id };
        let mut entry = MemoEntry::new();
        entry.value_id = Some(value_id);
        entry.last_verified = rev;
        entry.last_changed = rev;

        handler.event(Event::new(EventKind::RegisterMemo));
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .insert(memo_id, RwLock::new(entry));

        memo_id
    }

    pub(crate) fn memo_id<H>(&self, handler: &H, query_id: QueryId, args_id: S::Id) -> MemoId<S>
    where
        H: event::Handler,
    {
        let memo_id = MemoId { query_id, args_id };
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .entry(memo_id)
            .or_insert_with(|| {
                handler.event(Event::new(EventKind::RegisterMemo));
                RwLock::new(MemoEntry::new())
            });
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

    pub(crate) fn transfer(&mut self, capacity: usize) -> MemosTransfer<'_, S> {
        let memos = self.0.get_mut().expect(INCONSISTENT_STATE);
        let prev = mem::replace(memos, HashMap::with_capacity(capacity));
        MemosTransfer { prev, next: memos }
    }
}

impl<S> MemosTransfer<'_, S>
where
    S: Storage,
{
    pub(crate) fn retain<F>(&mut self, memo_id: MemoId<S>, transfer: F)
    where
        F: FnOnce(S::Id) -> (MemoId<S>, S::Id),
    {
        let mut memo = self.prev.remove(&memo_id).expect(UNKNOWN_ID);
        let entry = memo.get_mut().expect(INCONSISTENT_STATE);
        let value_id = entry
            .value_id
            .expect("bug: transferred memo has no stored value");
        let (next_memo_id, next_value_id) = transfer(value_id);
        entry.value_id = Some(next_value_id);
        self.next.insert(next_memo_id, memo);
    }

    pub(crate) fn finish<H>(self, handler: &H)
    where
        H: event::Handler,
    {
        let Self { prev, next: _next } = self;
        for _ in 0..prev.len() {
            handler.event(Event::new(EventKind::DeregisterMemo));
        }
    }
}

impl<S> Default for Memos<S>
where
    S: Storage,
{
    fn default() -> Self {
        Self(RwLock::default())
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
    fn hash<H>(&self, state: &mut H)
    where
        H: Hasher,
    {
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
