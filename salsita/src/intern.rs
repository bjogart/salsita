use crate::INCONSISTENT_STATE;
use crate::query::Input;
use crate::query::InputId;
use alloc::sync::Arc;
use core::any::Any;
use core::hash::BuildHasher as _;
use core::hash::Hash;
use core::num::NonZeroUsize;
use std::collections::HashMap;
use std::hash::RandomState;
use std::sync::RwLock;

const UNKNOWN_ID: &str = "bug: unknown intern ID (was this ID created by another database?)";

#[derive(Debug, Default)]
pub(crate) struct Interner(RwLock<InternerInner>);

#[derive(Debug, Default)]
struct InternerInner {
    fingerprint_hasher: FingerprintHasher,
    index: HashMap<Fingerprint, Bucket>,
    values: Vec<Arc<dyn Any + Send + Sync>>,
}

type Fingerprint = u64;

type FingerprintHasher = RandomState;

type Bucket = Vec<InternId>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct InternId(NonZeroUsize);

impl Interner {
    pub(crate) fn intern_input_id<I>(&self, f: impl FnOnce(InternId) -> InputId<I>) -> InputId<I>
    where
        I: Input,
    {
        let idx = self.0.read().expect(INCONSISTENT_STATE).values.len();
        let input_id = f(InternId::new(idx));
        self.intern(&input_id);
        input_id
    }

    pub(crate) fn intern<T>(&self, value: &T) -> InternId
    where
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let mut inner = self.0.write().expect(INCONSISTENT_STATE);
        let InternerInner {
            fingerprint_hasher,
            index,
            values,
        } = &mut *inner;
        let bucket = Self::find_bucket(fingerprint_hasher, index, value);
        Self::find_bucket_entry::<T>(bucket, values, value)
            .unwrap_or_else(|| Self::insert_value(bucket, values, value))
    }

    fn find_bucket<'index, T>(
        hash_builder: &FingerprintHasher,
        index: &'index mut HashMap<Fingerprint, Bucket>,
        value: &T,
    ) -> &'index mut Bucket
    where
        T: Hash + 'static,
    {
        let print = hash_builder.hash_one(value);
        index.entry(print).or_default()
    }

    fn find_bucket_entry<T>(
        bucket: &Bucket,
        values: &[Arc<dyn Any + Send + Sync>],
        value: &T,
    ) -> Option<InternId>
    where
        T: Eq + 'static,
    {
        bucket.iter().find_map(|id| {
            if let Some(stored) = Self::get_ref(values, *id).downcast_ref::<T>()
                && value == stored
            {
                return Some(*id);
            }
            None
        })
    }

    fn insert_value<T>(
        bucket: &mut Bucket,
        values: &mut Vec<Arc<dyn Any + Send + Sync>>,
        value: &T,
    ) -> InternId
    where
        T: Clone + Send + Sync + 'static,
    {
        let id = InternId::new(values.len());
        values.push(Arc::new(value.clone()));
        bucket.push(id);
        id
    }

    pub(crate) fn get(&self, id: InternId) -> Arc<dyn Any + Send + Sync> {
        Arc::clone(Self::get_ref(
            &self.0.read().expect(INCONSISTENT_STATE).values,
            id,
        ))
    }

    fn get_ref(values: &[Arc<dyn Any + Send + Sync>], id: InternId) -> &Arc<dyn Any + Send + Sync> {
        values.get(id.idx()).expect(UNKNOWN_ID)
    }
}

impl InternId {
    const fn new(idx: usize) -> Self {
        Self(NonZeroUsize::new(idx + 1).expect("bug: interner ID overflow"))
    }

    const fn idx(self) -> usize {
        self.0.get() - 1
    }
}
