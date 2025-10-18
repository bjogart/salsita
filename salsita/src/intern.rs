use crate::Db;
use crate::Revision;
use crate::metrics::Metrics;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use core::any::Any;
use core::any::TypeId;
use core::fmt::Debug;
use core::hash::BuildHasher as _;
use core::hash::Hash;
use std::collections::HashMap;
use std::hash::RandomState;

const NO_SUCH_ITEM: &str =
    "`Id` not in in Interner. This is probably due to cross-contamination from multiple `Db`s.";

#[derive(Debug, Default)]
pub(crate) struct MemoData<M> {
    print_hasher: FingerprintHasher,
    args_index: HashMap<Fingerprint, Bucket, FingerprintHasher>,
    args_items: Vec<AnyValue>,
    memos: HashMap<MemoId, MemoEntry<M>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Fingerprint(u64);

type FingerprintHasher = RandomState;

#[derive(Debug, Default)]
struct Bucket(Vec<ArgsId>);

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct MemoId {
    query: TypeId,
    args_id: ArgsId,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct ArgsId {
    idx: usize,
}

#[derive(Debug)]
pub(crate) struct MemoEntry<M> {
    pub(crate) deps: Vec<MemoId>,
    pub(crate) last_verified: Revision,
    pub(crate) eval: fn(&Db<M>, &dyn Any) -> AnyValue,
    pub(crate) value: Option<AnyValue>,
}

#[derive(Debug)]
pub(crate) struct AnyValue(Box<dyn Any>);

impl<M> MemoData<M>
where
    M: Metrics,
{
    pub(crate) fn new_input<I>(&mut self, rev: Revision, value: I::Value) -> InputId<I>
    where
        I: Input,
    {
        let args_id = ArgsId {
            idx: self.args_items.len(),
        };
        let memo_id = MemoId {
            query: TypeId::of::<I>(),
            args_id,
        };
        let input_id = InputId::from(memo_id);
        let dup_args_id = self.intern_args(&input_id);
        debug_assert_eq!(args_id, dup_args_id);

        let mut entry = MemoEntry::new::<I>();
        entry.set_value::<I>(rev, value);
        self.memos.insert(memo_id, entry);

        input_id
    }

    pub(crate) fn intern_memo<Q>(&mut self, args: &Q::Args) -> MemoId
    where
        Q: Query,
    {
        let args_id = self.intern_args(args);
        let memo_id = MemoId {
            query: TypeId::of::<Q>(),
            args_id,
        };
        self.memos
            .entry(memo_id)
            .or_insert_with(MemoEntry::new::<Q>);
        memo_id
    }

    fn intern_args<A>(&mut self, args: &A) -> ArgsId
    where
        A: Clone + Eq + Hash + 'static,
    {
        let bucket = Self::args_bucket(&self.print_hasher, &mut self.args_index, args);
        match Self::bucket_entry::<A>(&self.args_items, bucket, args) {
            Some(id) => id,
            None => Self::insert_args(&mut self.args_items, bucket, args),
        }
    }

    fn args_bucket<'index, A>(
        hash_builder: &FingerprintHasher,
        args_index: &'index mut HashMap<Fingerprint, Bucket>,
        args: &A,
    ) -> &'index mut Bucket
    where
        A: Hash + 'static,
    {
        let print = Fingerprint::new(hash_builder, args);
        args_index.entry(print).or_default()
    }

    fn bucket_entry<A>(args_items: &[AnyValue], bucket: &Bucket, args: &A) -> Option<ArgsId>
    where
        A: Eq + 'static,
    {
        bucket.0.iter().find_map(|id| {
            let stored = args_items.get(id.idx).expect(NO_SUCH_ITEM).downcast::<A>();
            (args == stored).then_some(*id)
        })
    }

    fn insert_args<A>(args_items: &mut Vec<AnyValue>, bucket: &mut Bucket, args: &A) -> ArgsId
    where
        A: Clone + Eq + Hash + 'static,
    {
        let idx = args_items.len();
        let id = ArgsId { idx };
        args_items.push(AnyValue::new::<A>(args.clone()));
        bucket.0.push(id);
        id
    }

    pub(crate) fn memo_mut(&mut self, id: MemoId) -> &mut MemoEntry<M> {
        self.memos.get_mut(&id).expect(NO_SUCH_ITEM)
    }

    pub(crate) fn memo(&self, id: MemoId) -> &MemoEntry<M> {
        self.memos.get(&id).expect(NO_SUCH_ITEM)
    }
}

impl Fingerprint {
    fn new<T>(hash_builder: &FingerprintHasher, value: &T) -> Self
    where
        T: Hash + 'static,
    {
        Self(hash_builder.hash_one((TypeId::of::<T>(), value)))
    }
}

impl<M> MemoEntry<M>
where
    M: Metrics,
{
    const fn new<Q>() -> Self
    where
        Q: Query,
    {
        return Self {
            deps: Vec::new(),
            last_verified: Revision::NEVER_VERIFIED,
            eval: eval::<M, Q>,
            value: None,
        };

        fn eval<M, Q>(db: &Db<M>, args: &dyn Any) -> AnyValue
        where
            M: Metrics,
            Q: Query,
        {
            let args = args
                .downcast_ref()
                .unwrap_or_else(|| panic!("type cast failed"));
            let out = Q::eval(db, args);
            AnyValue::new::<Q::Out>(out)
        }
    }

    pub(crate) fn set_value<Q>(&mut self, rev: Revision, value: Q::Out)
    where
        Q: Query,
    {
        self.value = Some(AnyValue::new::<Q::Out>(value));
        self.last_verified = rev;
    }
}

impl AnyValue {
    fn new<T>(value: T) -> Self
    where
        T: 'static,
    {
        Self(Box::new(value))
    }

    pub(crate) fn downcast<T>(&self) -> &T
    where
        T: 'static,
    {
        self.0
            .downcast_ref()
            .unwrap_or_else(|| panic!("type cast failed"))
    }
}
