use crate::ActiveQuery;
use crate::ActiveQueryStack;
use crate::Revision;
use crate::Snapshot;
use crate::memo::MemoId;
use crate::memo::Memos;
use crate::storage::Storage;

pub(crate) struct QueryUpdate<'snap, S, H>
where
    S: Storage,
{
    snapshot: &'snap Snapshot<S, H>,
    pub(crate) commit: PendingCommit<S>,
}

pub(crate) struct MemoUpdate<'memos, S>
where
    S: Storage,
{
    memos: &'memos Memos<S>,
    commit: PendingCommit<S>,
}

pub(crate) struct PendingCommit<S>
where
    S: Storage,
{
    current_rev: Revision,
    memo_id: MemoId<S>,
    deps: Option<Vec<MemoId<S>>>,
    pub(crate) new_value_id: Option<S::Id>,
}

impl<'snap, S, H> QueryUpdate<'snap, S, H>
where
    S: Storage,
{
    pub(crate) const fn new(snapshot: &'snap Snapshot<S, H>, commit: PendingCommit<S>) -> Self {
        Self { snapshot, commit }
    }
}

impl<S, H> Drop for QueryUpdate<'_, S, H>
where
    S: Storage,
{
    fn drop(&mut self) {
        let Self { snapshot, commit } = self;
        let mut active_queries = snapshot.active_queries.borrow_mut();
        let ActiveQueryStack(active_queries) = &mut *active_queries;
        if let Some(ActiveQuery { deps }) = active_queries.pop() {
            commit.deps = Some(deps);
        }
        let _update = MemoUpdate::new(&snapshot.db.global.memos, commit.take());
    }
}

impl<'memos, S> MemoUpdate<'memos, S>
where
    S: Storage,
{
    pub(crate) const fn new(memos: &'memos Memos<S>, commit: PendingCommit<S>) -> Self {
        Self { memos, commit }
    }
}

impl<S> Drop for MemoUpdate<'_, S>
where
    S: Storage,
{
    fn drop(&mut self) {
        let Self {
            memos,
            commit:
                PendingCommit {
                    current_rev,
                    memo_id,
                    deps,
                    new_value_id,
                },
        } = self;
        memos.memo_mut(*memo_id, |memo| {
            memo.last_verified = *current_rev;
            if let Some(deps) = deps.take() {
                memo.deps = deps;
            }
            if let Some(value_id) = new_value_id.take() {
                memo.value_id = Some(value_id);
                memo.last_changed = *current_rev;
            }
        });
    }
}

impl<S> PendingCommit<S>
where
    S: Storage,
{
    pub(crate) const fn new(current_rev: Revision, memo_id: MemoId<S>) -> Self {
        Self {
            current_rev,
            memo_id,
            deps: None,
            new_value_id: None,
        }
    }

    const fn take(&mut self) -> Self {
        Self {
            current_rev: self.current_rev,
            memo_id: self.memo_id,
            deps: self.deps.take(),
            new_value_id: self.new_value_id.take(),
        }
    }
}
