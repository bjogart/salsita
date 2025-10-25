use crate::query::Input;
use crate::query::InputId;
use alloc::rc::Rc;
use core::any::Any;
use core::hash::BuildHasher as _;
use core::hash::Hash;
use std::collections::HashMap;
use std::hash::RandomState;

const UNKNOWN_ID: &str = "bug: unknown intern ID (was this ID created by another database?)";

#[derive(Debug, Default)]
pub(crate) struct Interner {
    print_hasher: FingerprintHasher,
    index: HashMap<Fingerprint, Bucket>,
    values: Vec<Rc<dyn Any>>,
}

#[derive(Debug, PartialEq, Eq, Hash)]
struct Fingerprint(u64);

type FingerprintHasher = RandomState;

#[derive(Debug, Default)]
struct Bucket(Vec<InternId>);

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
        T: Clone + Eq + Hash + 'static,
    {
        let bucket = Self::find_bucket(&self.print_hasher, &mut self.index, value);
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
        let print = Fingerprint::new(hash_builder, value);
        index.entry(print).or_default()
    }

    fn find_bucket_entry<T>(bucket: &Bucket, values: &[Rc<dyn Any>], value: &T) -> Option<InternId>
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

    fn insert_value<T>(bucket: &mut Bucket, values: &mut Vec<Rc<dyn Any>>, value: &T) -> InternId
    where
        T: Clone + 'static,
    {
        let idx = values.len();
        let id = InternId { idx };
        values.push(Rc::new(value.clone()));
        bucket.0.push(id);
        id
    }

    pub(crate) fn interned(&self, id: InternId) -> Rc<dyn Any> {
        Rc::clone(Self::interned_ref(&self.values, id))
    }

    fn interned_ref(values: &[Rc<dyn Any>], id: InternId) -> &Rc<dyn Any> {
        values.get(id.idx).expect(UNKNOWN_ID)
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
