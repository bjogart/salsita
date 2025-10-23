use crate::Db;
use crate::Revision;
use crate::metrics::Metrics;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use alloc::rc::Rc;
use core::any::Any;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::BuildHasher as _;
use core::hash::Hash;
use std::collections::HashMap;
use std::hash::RandomState;

const NO_SUCH_ITEM: &str = "`Id` not in in `MemoData`";
const TYPE_CAST_FAILED: &str = "type cast failed";

#[derive(Debug, Default)]
pub(crate) struct MemoData<M> {
    print_hasher: FingerprintHasher,
    index: HashMap<Fingerprint, Bucket>,
    values: Vec<Rc<dyn Any>>,
    memos: HashMap<MemoId, MemoEntry<M>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Fingerprint(u64);

type FingerprintHasher = RandomState;

#[derive(Debug, Default)]
struct Bucket(Vec<InternId>);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct MemoId {
    query: TypeId,
    args: InternId,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct InternId {
    idx: usize,
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

impl<M> MemoData<M>
where
    M: Metrics,
{
    pub(crate) fn new_input<I>(&mut self, rev: Revision, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let idx = self.values.len();
        let args = InternId { idx };
        let query = TypeId::of::<I>();
        let memo_id = MemoId { query, args };
        let input_id = InputId::from(memo_id);
        let _args = self.intern_value(&input_id);
        debug_assert_eq!(args, _args);

        let mut entry = MemoEntry::new::<I>();
        entry.value = Some(Box::new(value));
        entry.last_verified = rev;
        entry.last_changed = rev;
        self.memos.insert(memo_id, entry);

        input_id
    }

    pub(crate) fn intern_query<Q>(&mut self, args: &Q::Args) -> MemoId
    where
        Q: Query,
    {
        let query = TypeId::of::<Q>();
        let args = self.intern_value(args);
        let memo_id = MemoId { query, args };
        self.memos
            .entry(memo_id)
            .or_insert_with(MemoEntry::new::<Q>);
        memo_id
    }

    fn intern_value<T>(&mut self, value: &T) -> InternId
    where
        T: Clone + Eq + Hash + 'static,
    {
        let bucket = Self::find_bucket(&self.print_hasher, &mut self.index, value);
        match Self::intern_id::<T>(bucket, &self.values, value) {
            Some(id) => id,
            None => Self::insert_value_in_bucket(bucket, &mut self.values, value),
        }
    }

    fn find_bucket<'index, T>(
        hash_builder: &FingerprintHasher,
        index: &'index mut HashMap<Fingerprint, Bucket>,
        value: &T,
    ) -> &'index mut Bucket
    where
        T: Hash + 'static,
    {
        let print = Fingerprint::new(hash_builder, value);
        index.entry(print).or_default()
    }

    fn intern_id<T>(bucket: &Bucket, values: &[Rc<dyn Any>], value: &T) -> Option<InternId>
    where
        T: Eq + 'static,
    {
        bucket.0.iter().find_map(|id| {
            if let Some(stored) = Self::interned_ref(values, *id).downcast_ref::<T>()
                && value == stored
            {
                return Some(*id);
            }
            None
        })
    }

    fn insert_value_in_bucket<T>(
        bucket: &mut Bucket,
        values: &mut Vec<Rc<dyn Any>>,
        value: &T,
    ) -> InternId
    where
        T: Clone + 'static,
    {
        let idx = values.len();
        let id = InternId { idx };
        values.push(Rc::new(value.clone()));
        bucket.0.push(id);
        id
    }

    pub(crate) fn memo_mut(&mut self, id: MemoId) -> &mut MemoEntry<M> {
        self.memos.get_mut(&id).expect(NO_SUCH_ITEM)
    }

    pub(crate) fn memo(&self, id: MemoId) -> &MemoEntry<M> {
        self.memos.get(&id).expect(NO_SUCH_ITEM)
    }

    pub(crate) fn interned(&self, id: InternId) -> Rc<dyn Any> {
        Rc::clone(Self::interned_ref(&self.values, id))
    }

    fn interned_ref(values: &[Rc<dyn Any>], id: InternId) -> &Rc<dyn Any> {
        values.get(id.idx).expect(NO_SUCH_ITEM)
    }
}

impl Fingerprint {
    fn new<T>(hash_builder: &FingerprintHasher, value: &T) -> Self
    where
        T: Hash + 'static,
    {
        Self(hash_builder.hash_one(value))
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
        self.args
    }
}
