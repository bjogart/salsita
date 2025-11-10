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
    pub(crate) change: Option<PendingChange<S>>,
}

pub(crate) struct PendingChange<S>
where
    S: Storage,
{
    value_id: S::Id,
    deps: Option<Vec<MemoId<S>>>,
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
        if let Some(change) = commit.change.as_mut()
            && let Some(ActiveQuery { deps }) = active_queries.pop()
        {
            change.deps = Some(deps);
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
                    change,
                },
        } = self;
        memos.memo_mut(*memo_id, |memo| {
            memo.last_verified = *current_rev;
            if let Some(PendingChange { deps, value_id }) = change.take() {
                memo.value_id = Some(value_id);
                memo.last_changed = *current_rev;
                if let Some(deps) = deps {
                    memo.deps = deps;
                }
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
            change: None,
        }
    }

    const fn take(&mut self) -> Self {
        Self {
            change: self.change.take(),
            current_rev: self.current_rev,
            memo_id: self.memo_id,
        }
    }
}

impl<S> PendingChange<S>
where
    S: Storage,
{
    pub(crate) const fn new(value_id: S::Id) -> Self {
        Self {
            value_id,
            deps: None,
        }
    }
}
