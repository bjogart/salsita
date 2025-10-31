use crate::query::Input;
use crate::query::InputId;
use alloc::sync::Arc;
use core::any::Any;
use core::hash::BuildHasher as _;
use core::hash::Hash;
use std::collections::HashMap;
use std::hash::RandomState;

const UNKNOWN_ID: &str = "bug: unknown intern ID (was this ID created by another database?)";

#[derive(Debug, Default)]
pub(crate) struct Interner {
    fingerprint_hasher: FingerprintHasher,
    index: HashMap<Fingerprint, Bucket>,
    values: Vec<Arc<dyn Any + Send + Sync>>,
}

type Fingerprint = u64;

type FingerprintHasher = RandomState;

type Bucket = Vec<InternId>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub(crate) struct InternId {
    idx: usize,
}

impl Interner {
    pub(crate) fn intern_input_id<I>(
        &mut self,
        f: impl FnOnce(InternId) -> InputId<I>,
    ) -> InputId<I>
    where
        I: Input,
    {
        let idx = self.values.len();
        let input_id = f(InternId { idx });
        self.intern(&input_id);
        input_id
    }

    pub(crate) fn intern<T>(&mut self, value: &T) -> InternId
    where
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let bucket = Self::find_bucket(&self.fingerprint_hasher, &mut self.index, value);
        match Self::find_bucket_entry::<T>(bucket, &self.values, value) {
            Some(id) => id,
            None => Self::insert_value(bucket, &mut self.values, value),
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
        let idx = values.len();
        let id = InternId { idx };
        values.push(Arc::new(value.clone()));
        bucket.push(id);
        id
    }

    pub(crate) fn get(&self, id: InternId) -> Arc<dyn Any + Send + Sync> {
        Arc::clone(Self::get_ref(&self.values, id))
    }

    fn get_ref(values: &[Arc<dyn Any + Send + Sync>], id: InternId) -> &Arc<dyn Any + Send + Sync> {
        values.get(id.idx).expect(UNKNOWN_ID)
    }
}
