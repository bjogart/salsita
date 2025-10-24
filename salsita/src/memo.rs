use crate::Db;
use crate::Revision;
use crate::intern::InternId;
use crate::metrics::Metrics;
use crate::query::Input;
use crate::query::Query;
use core::any::Any;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::Hash;
use std::collections::HashMap;

const UNKNOWN_ID: &str = "`MemoId` not in in `Memos`";
const TYPE_CAST_FAILED: &str = "type cast failed";

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
    pub(crate) eval: fn(db: &Db<M>, args: &dyn Any) -> Box<dyn Any>,
    pub(crate) eq: fn(a: &dyn Any, b: &dyn Any) -> bool,
    pub(crate) deps: Vec<MemoId>,
    pub(crate) last_verified: Revision,
    pub(crate) last_changed: Revision,
    pub(crate) value: Option<Box<dyn Any>>,
}

impl<M> Memos<M>
where
    M: Metrics,
{
    pub(crate) fn new_input<I>(
        &mut self,
        rev: Revision,
        args_id: InternId,
        value: I::Value,
    ) -> MemoId
    where
        I: Input,
    {
        let query = TypeId::of::<I>();
        let memo_id = MemoId { query, args_id };
        let mut entry = MemoEntry::new::<I>();
        entry.value = Some(Box::new(value));
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
            eq: eq::<Q::Out>,
            deps: Vec::new(),
            last_verified: Revision::NEVER_VERIFIED,
            last_changed: Revision::NEVER_VERIFIED,
            value: None,
        };

        fn eval<M, Q>(db: &Db<M>, args: &dyn Any) -> Box<dyn Any>
        where
            M: Metrics,
            Q: Query,
        {
            let args = args.downcast_ref().expect(TYPE_CAST_FAILED);
            let out = Q::eval(db, args);
            Box::new(out)
        }

        fn eq<T>(a: &dyn Any, b: &dyn Any) -> bool
        where
            T: Eq + 'static,
        {
            a.downcast_ref::<T>().expect(TYPE_CAST_FAILED)
                == b.downcast_ref::<T>().expect(TYPE_CAST_FAILED)
        }
    }

    pub(crate) fn value<T>(&self) -> &T
    where
        T: 'static,
    {
        self.value
            .as_ref()
            .expect("value not memoized")
            .downcast_ref()
            .expect(TYPE_CAST_FAILED)
    }
}

impl MemoId {
    pub(crate) const fn args(self) -> InternId {
        self.args_id
    }
}
