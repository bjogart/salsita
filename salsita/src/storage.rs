use crate::INCONSISTENT_STATE;
use crate::panic_expected_different_type;
use alloc::sync::Arc;
use core::any::Any;
use core::fmt::Debug;
use core::hash::BuildHasher as _;
use core::hash::Hash;
use core::num::NonZeroUsize;
use core::ops::Deref;
use std::collections::HashMap;
use std::hash::RandomState;
use std::sync::RwLock;

const UNKNOWN_ID: &str = "bug: unknown storage ID (was this ID created by another database?)";

pub trait Storage: Default + 'static {
    type Id: Clone + Copy + Eq + Hash + Debug + Send + Sync;
    type Value: Debug + Downcast;

    fn store<T>(&self, value: &T) -> Self::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static;

    fn get(&self, id: Self::Id) -> Self::Value;
}

pub trait Downcast {
    type Downcast<T>: Deref<Target = T>;

    fn downcast<T>(self) -> Self::Downcast<T>
    where
        T: Send + Sync + 'static;
}

#[derive(Debug, Default)]
pub struct DefaultStorage(RwLock<DefaultStorageInner>);

#[derive(Debug, Default)]
struct DefaultStorageInner {
    fingerprint_hasher: FingerprintHasher,
    index: HashMap<Fingerprint, Bucket>,
    values: Vec<Arc<dyn Any + Send + Sync>>,
}

type Fingerprint = u64;

type FingerprintHasher = RandomState;

type Bucket = Vec<DefaultStorageId>;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct DefaultStorageId(NonZeroUsize);

impl Storage for DefaultStorage {
    type Id = DefaultStorageId;

    type Value = Arc<dyn Any + Send + Sync>;

    fn store<T>(&self, value: &T) -> Self::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let mut inner = self.0.write().expect(INCONSISTENT_STATE);
        let DefaultStorageInner {
            fingerprint_hasher,
            index,
            values,
        } = &mut *inner;
        let bucket = Self::find_bucket(fingerprint_hasher, index, value);
        Self::find_bucket_entry::<T>(bucket, values, value)
            .unwrap_or_else(|| Self::insert_value(bucket, values, value))
    }

    fn get(&self, id: Self::Id) -> Self::Value {
        Arc::clone(Self::get_ref(
            &self.0.read().expect(INCONSISTENT_STATE).values,
            id,
        ))
    }
}

impl DefaultStorage {
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
    ) -> Option<DefaultStorageId>
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
    ) -> DefaultStorageId
    where
        T: Clone + Send + Sync + 'static,
    {
        let id = DefaultStorageId::new(values.len());
        values.push(Arc::new(value.clone()));
        bucket.push(id);
        id
    }

    fn get_ref(
        values: &[Arc<dyn Any + Send + Sync>],
        id: DefaultStorageId,
    ) -> &Arc<dyn Any + Send + Sync> {
        values.get(id.idx()).expect(UNKNOWN_ID)
    }
}

impl DefaultStorageId {
    const fn new(idx: usize) -> Self {
        Self(NonZeroUsize::new(idx + 1).expect("bug: storage ID overflow"))
    }

    const fn idx(self) -> usize {
        self.0.get() - 1
    }
}

impl Downcast for Arc<dyn Any + Send + Sync> {
    type Downcast<T> = Arc<T>;

    fn downcast<T>(self) -> Self::Downcast<T>
    where
        T: Send + Sync + 'static,
    {
        self.downcast()
            .unwrap_or_else(|_| panic_expected_different_type::<T>())
    }
}
