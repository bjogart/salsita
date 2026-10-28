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
    pub(crate) commit: Option<PendingCommit<S>>,
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
    pub(crate) const fn new(snapshot: &'snap Snapshot<S, H>) -> Self {
        Self {
            snapshot,
            commit: None,
        }
    }
}

impl<S, H> Drop for QueryUpdate<'_, S, H>
where
    S: Storage,
{
    fn drop(&mut self) {
        let Self { snapshot, commit } = self;
        let deps = {
            let mut active_queries = snapshot.active_queries.borrow_mut();
            let ActiveQueryStack(active_queries) = &mut *active_queries;
            active_queries.pop().map(|ActiveQuery { deps }| deps)
        };
        if let Some(mut commit) = commit.take() {
            commit.deps = deps;
            commit.apply(&snapshot.shared.global.memos);
        }
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

    pub(crate) fn apply(self, memos: &Memos<S>) {
        let Self {
            current_rev,
            memo_id,
            deps,
            new_value_id,
        } = self;
        memos.memo_mut(memo_id, |memo| {
            memo.last_verified = current_rev;
            if let Some(deps) = deps {
                memo.deps = deps;
            }
            if let Some(value_id) = new_value_id {
                memo.value_id = Some(value_id);
                memo.last_changed = current_rev;
            }
        });
    }
}
