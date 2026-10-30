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
use core::ops::Deref;
use std::collections::HashMap;
use std::hash::RandomState;
use std::sync::RwLock;
use std::sync::RwLockReadGuard;

pub trait Storage: Default + 'static {
    type Id: Clone + Copy + Eq + Hash + Debug + Send + Sync;
    type Handle: Debug + Handle;
    type Transfer<'storage>: Transfer<Storage = Self>
    where
        Self: 'storage;

    fn store<H, T>(&self, handler: &H, value: &T) -> Self::Id
    where
        H: event::Handler,
        T: Clone + Eq + Hash + Send + Sync + 'static;

    fn get(&self, id: Self::Id) -> Self::Handle;

    fn transfer(&self) -> Self::Transfer<'_>;
}

pub trait Transfer {
    type Storage: Storage;

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
pub struct DefaultTransfer<'storage> {
    curr: RwLockReadGuard<'storage, DefaultStorageInner>,
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
pub struct DefaultStorageId(usize);

impl Storage for DefaultStorage {
    type Id = DefaultStorageId;

    type Handle = Arc<dyn Any + Send + Sync>;

    type Transfer<'storage> = DefaultTransfer<'storage>;

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

    fn transfer(&self) -> Self::Transfer<'_> {
        DefaultTransfer {
            curr: self.0.read().expect(INCONSISTENT_STATE),
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
        self.intern(value, || {
            handler.event(Event::new(EventKind::StoreValue));
            Arc::new(value.clone())
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

    fn intern<T>(
        &mut self,
        value: &T,
        make_shared: impl FnOnce() -> Arc<dyn Any + Send + Sync>,
    ) -> DefaultStorageId
    where
        T: Eq + Hash + 'static,
    {
        let Self {
            fingerprint_hasher,
            index,
            values,
        } = self;
        let bucket = Self::find_bucket(fingerprint_hasher, index, value);
        Self::find_bucket_entry::<T>(bucket, values, value).unwrap_or_else(|| {
            let id = DefaultStorageId::new(values.len());
            values.push(make_shared());
            bucket.push(id);
            id
        })
    }

    fn get(&self, id: DefaultStorageId) -> Arc<dyn Any + Send + Sync> {
        Arc::clone(Self::get_ref(&self.values, id))
    }

    fn get_ref(
        values: &[Arc<dyn Any + Send + Sync>],
        id: DefaultStorageId,
    ) -> &Arc<dyn Any + Send + Sync> {
        values
            .get(id.idx())
            .expect("bug: unknown storage ID (was this ID created by another database?)")
    }
}

impl Transfer for DefaultTransfer<'_> {
    type Storage = DefaultStorage;

    fn transfer<T>(&mut self, id: <Self::Storage as Storage>::Id) -> <Self::Storage as Storage>::Id
    where
        T: Clone + Eq + Hash + Send + Sync + 'static,
    {
        let shared = self.curr.get(id);
        let Some(value) = shared.downcast_ref::<T>() else {
            panic_expected_different_type::<T>()
        };
        self.next.intern(value, || Arc::clone(&shared))
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
        Self(idx)
    }

    const fn idx(self) -> usize {
        self.0
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
