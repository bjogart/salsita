use crate::Revision;
use crate::Snapshot;
use crate::intern::InternId;
use crate::intern::Interner;
use crate::metrics::Metrics;
use crate::panic_expected_different_type;
use crate::query::Input;
use crate::query::Query;
use core::any::Any;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::Hash;
use std::collections::HashMap;

const UNKNOWN_ID: &str = "bug: unknown memo ID (was this ID created by another database?)";

#[derive(Debug, Default)]
pub(crate) struct Memos<M> {
    memos: HashMap<MemoId, MemoEntry<M>>,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct MemoId {
    query: TypeId,
    args_id: InternId,
}

#[derive(Debug)]
pub(crate) struct MemoEntry<M> {
    pub(crate) eval:
        fn(snapshot: &Snapshot<M>, args: &(dyn Any + Send + Sync)) -> Box<dyn Any + Send + Sync>,
    pub(crate) intern_output:
        fn(interner: &mut Interner, value: &(dyn Any + Send + Sync)) -> InternId,
    pub(crate) cancelable: bool,
    pub(crate) deps: Vec<MemoId>,
    pub(crate) last_verified: Revision,
    pub(crate) last_changed: Revision,
    pub(crate) value_id: Option<InternId>,
}

impl<M> Memos<M>
where
    M: Metrics,
{
    pub(crate) fn new_input<I>(
        &mut self,
        rev: Revision,
        args_id: InternId,
        value_id: InternId,
    ) -> MemoId
    where
        I: Input,
    {
        let query = TypeId::of::<I>();
        let memo_id = MemoId { query, args_id };
        let mut entry = MemoEntry::new::<I>();
        entry.value_id = Some(value_id);
        entry.last_verified = rev;
        entry.last_changed = rev;
        self.memos.insert(memo_id, entry);
        memo_id
    }

    pub(crate) fn intern<Q>(&mut self, args_id: InternId) -> MemoId
    where
        Q: Query,
    {
        let query = TypeId::of::<Q>();
        let memo_id = MemoId { query, args_id };
        self.memos
            .entry(memo_id)
            .or_insert_with(MemoEntry::new::<Q>);
        memo_id
    }

    pub(crate) fn memo_mut(&mut self, id: MemoId) -> &mut MemoEntry<M> {
        self.memos.get_mut(&id).expect(UNKNOWN_ID)
    }

    pub(crate) fn memo(&self, id: MemoId) -> &MemoEntry<M> {
        self.memos.get(&id).expect(UNKNOWN_ID)
    }
}

impl<M> MemoEntry<M>
where
    M: Metrics,
{
    fn new<Q>() -> Self
    where
        Q: Query,
    {
        return Self {
            eval: eval::<M, Q>,
            intern_output: intern_output::<Q::Out>,
            cancelable: Q::canceled().is_some(),
            deps: Vec::new(),
            last_verified: Revision::NEVER_VERIFIED,
            last_changed: Revision::NEVER_VERIFIED,
            value_id: None,
        };

        fn eval<M, Q>(
            snapshot: &Snapshot<M>,
            args: &(dyn Any + Send + Sync),
        ) -> Box<dyn Any + Send + Sync>
        where
            M: Metrics,
            Q: Query,
        {
            let Some(args) = args.downcast_ref::<Q::Args>() else {
                panic_expected_different_type::<&Q::Args>()
            };
            let out = Q::eval(snapshot, args);
            Box::new(out)
        }

        fn intern_output<T>(interner: &mut Interner, out: &(dyn Any + Send + Sync)) -> InternId
        where
            T: Clone + Eq + Hash + Send + Sync + 'static,
        {
            let Some(out) = out.downcast_ref::<T>() else {
                panic_expected_different_type::<&T>()
            };
            interner.intern(out)
        }
    }
}

impl MemoId {
    pub(crate) const fn args(self) -> InternId {
        self.args_id
    }
}
