use crate::INCONSISTENT_STATE;
use crate::event;
use crate::event::Event;
use crate::event::EventKind;
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
    type Handle: Debug + Handle;
    type Transfer: Transfer<Storage = Self>;

    fn store<H, T>(&self, handler: &H, value: &T) -> Self::Id
    where
        H: event::Handler,
        T: Clone + Eq + Hash + Send + Sync + 'static;

    fn get(&self, id: Self::Id) -> Self::Handle;

    fn into_transfer(self) -> Self::Transfer;
}

pub trait Transfer {
    type Storage: Storage;

    /// Retrieve a value corresponding to a storage ID.
    ///
    /// `id` must be a value stored inside the storage that created this
    /// [`Transfer`] struct. Passing in a storage ID from a different storage
    /// constitutes a logic bug in Salsita and implementations should panic if
    /// an invalid storage ID is encountered.
    ///
    /// [`transfer`][Transfer::transfer] may move the data associated with `id`
    /// to a new storage. Calling `self.get_raw_input_id(id)` after
    /// `self.transfer(id)` therefore also constitutes a logic error in Salsita
    /// and implementations should panic to indicate its existence.
    fn get(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Handle;

    fn transfer<T>(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static;

    fn into_storage<H>(self, handler: &H) -> Self::Storage
    where
        H: event::Handler;
}

pub trait Handle {
    type TypedHandle<T>: Deref<Target = T>;

    fn downcast<T>(self) -> Self::TypedHandle<T>
    where
        T: Send + Sync + 'static;
}

#[derive(Debug, Default)]
pub struct DefaultStorage(RwLock<DefaultStorageInner>);

#[derive(Debug)]
pub struct DefaultTransfer {
    curr: DefaultStorageInner,
    next: DefaultStorageInner,
}

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

    type Handle = Arc<dyn Any + Send + Sync>;

    type Transfer = DefaultTransfer;

    fn store<H, T>(&self, handler: &H, value: &T) -> Self::Id
    where
        H: event::Handler,
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let mut inner = self.0.write().expect(INCONSISTENT_STATE);
        inner.store(handler, value)
    }

    fn get(&self, id: Self::Id) -> Self::Handle {
        self.0.read().expect(INCONSISTENT_STATE).get(id)
    }

    fn into_transfer(self) -> Self::Transfer {
        DefaultTransfer {
            curr: self.0.into_inner().expect(INCONSISTENT_STATE),
            next: DefaultStorageInner::default(),
        }
    }
}

impl DefaultStorageInner {
    fn store<H, T>(&mut self, handler: &H, value: &T) -> DefaultStorageId
    where
        H: event::Handler,
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let Self {
            fingerprint_hasher,
            index,
            values,
        } = self;
        let bucket = Self::find_bucket(fingerprint_hasher, index, value);
        Self::find_bucket_entry::<T>(bucket, values, value).unwrap_or_else(|| {
            handler.event(Event::new(EventKind::StoreValue));
            Self::insert_value(bucket, values, value)
        })
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

    fn get(&self, id: DefaultStorageId) -> Arc<dyn Any + Send + Sync> {
        Arc::clone(Self::get_ref(&self.values, id))
    }

    fn get_ref(
        values: &[Arc<dyn Any + Send + Sync>],
        id: DefaultStorageId,
    ) -> &Arc<dyn Any + Send + Sync> {
        values.get(id.idx()).expect(UNKNOWN_ID)
    }
}

impl Transfer for DefaultTransfer {
    type Storage = DefaultStorage;

    fn get(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Handle {
        self.curr.get(id)
    }

    fn transfer<T>(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let value = Handle::downcast::<T>(self.curr.get(id));
        self.next.store(&(), value.as_ref())
    }

    fn into_storage<H>(self, handler: &H) -> Self::Storage
    where
        H: event::Handler,
    {
        let freed_values = self.curr.values.len() - self.next.values.len();
        handler.event(Event::new(EventKind::FreeValues(freed_values)));
        DefaultStorage(RwLock::new(self.next))
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

impl Handle for Arc<dyn Any + Send + Sync> {
    type TypedHandle<T> = Arc<T>;

    fn downcast<T>(self) -> Self::TypedHandle<T>
    where
        T: Send + Sync + 'static,
    {
        self.downcast()
            .unwrap_or_else(|_| panic_expected_different_type::<T>())
    }
}
