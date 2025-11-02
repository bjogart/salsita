use crate::INCONSISTENT_STATE;
use crate::Snapshot;
use crate::event;
use crate::panic_expected_different_type;
use crate::query::Query;
use crate::storage::DefaultStorage;
use crate::storage::DefaultStorageId;
use core::any::Any;
use core::any::TypeId;
use core::hash::Hash;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Default)]
pub(crate) struct QueryRegistry<H> {
    ops: RwLock<HashMap<TypeId, Ops<H>>>,
}

#[derive(Debug)]
pub(crate) struct Ops<H> {
    pub(crate) eval:
        fn(snapshot: &Snapshot<H>, args: &(dyn Any + Send + Sync)) -> Box<dyn Any + Send + Sync>,
    pub(crate) store_out:
        fn(storage: &DefaultStorage, value: &(dyn Any + Send + Sync)) -> DefaultStorageId,
}

impl<H> QueryRegistry<H>
where
    H: event::Handler,
{
    pub(crate) fn query_id<Q>(&self) -> TypeId
    where
        Q: Query,
    {
        let query_id = TypeId::of::<Q>();
        self.ops
            .write()
            .expect(INCONSISTENT_STATE)
            .entry(query_id)
            .or_insert_with(Ops::new::<Q>);
        query_id
    }

    pub(crate) fn get(&self, query_id: TypeId) -> Option<Ops<H>> {
        self.ops
            .read()
            .expect(INCONSISTENT_STATE)
            .get(&query_id)
            .copied()
    }
}

impl<H> Ops<H>
where
    H: event::Handler,
{
    fn new<Q>() -> Self
    where
        Q: Query,
    {
        return Self {
            eval: eval::<H, Q>,
            store_out: store_output::<Q::Out>,
        };

        fn eval<H, Q>(
            snapshot: &Snapshot<H>,
            args: &(dyn Any + Send + Sync),
        ) -> Box<dyn Any + Send + Sync>
        where
            H: event::Handler,
            Q: Query,
        {
            let Some(args) = args.downcast_ref::<Q::Args>() else {
                panic_expected_different_type::<&Q::Args>()
            };
            let out = Q::eval(snapshot, args);
            Box::new(out)
        }

        fn store_output<T>(
            storage: &DefaultStorage,
            out: &(dyn Any + Send + Sync),
        ) -> DefaultStorageId
        where
            T: Clone + Eq + Hash + Send + Sync + 'static,
        {
            let Some(out) = out.downcast_ref::<T>() else {
                panic_expected_different_type::<&T>()
            };
            storage.store(out)
        }
    }
}

impl<H> Clone for Ops<H> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<H> Copy for Ops<H> {}
