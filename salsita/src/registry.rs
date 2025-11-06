use crate::INCONSISTENT_STATE;
use crate::Snapshot;
use crate::event;
use crate::panic_expected_different_type;
use crate::query::Query;
use crate::storage::Storage;
use core::any::Any;
use core::any::TypeId;
use core::hash::Hash;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Default)]
pub(crate) struct QueryRegistry<S, H>
where
    S: Storage,
{
    ops: RwLock<HashMap<TypeId, Ops<S, H>>>,
}

#[derive(Debug)]
pub(crate) struct Ops<S, H>
where
    S: Storage,
{
    pub(crate) eval:
        fn(snapshot: &Snapshot<H>, args: &(dyn Any + Send + Sync)) -> Box<dyn Any + Send + Sync>,
    pub(crate) store_out: fn(storage: &S, value: &(dyn Any + Send + Sync)) -> S::Id,
}

impl<S, H> QueryRegistry<S, H>
where
    S: Storage,
    H: event::Handler,
{
    pub(crate) fn query_id<Q>(&self) -> TypeId
    where
        Q: Query<S>,
    {
        let query_id = TypeId::of::<Q>();
        self.ops
            .write()
            .expect(INCONSISTENT_STATE)
            .entry(query_id)
            .or_insert_with(Ops::new::<Q>);
        query_id
    }

    pub(crate) fn get(&self, query_id: TypeId) -> Option<Ops<S, H>> {
        self.ops
            .read()
            .expect(INCONSISTENT_STATE)
            .get(&query_id)
            .copied()
    }
}

impl<S, H> Ops<S, H>
where
    S: Storage,
    H: event::Handler,
{
    fn new<Q>() -> Self
    where
        Q: Query<S>,
    {
        return Self {
            eval: eval::<Q, H, S>,
            store_out: store_output::<S, Q::Out>,
        };

        fn eval<Q, H, S>(
            snapshot: &Snapshot<H>,
            args: &(dyn Any + Send + Sync),
        ) -> Box<dyn Any + Send + Sync>
        where
            H: event::Handler,
            Q: Query<S>,
            S: Storage,
        {
            let Some(args) = args.downcast_ref::<Q::Args>() else {
                panic_expected_different_type::<&Q::Args>()
            };
            let out = Q::eval(snapshot, args);
            Box::new(out)
        }

        fn store_output<S, T>(storage: &S, out: &(dyn Any + Send + Sync)) -> S::Id
        where
            S: Storage,
            T: Clone + Eq + Hash + Send + Sync + 'static,
        {
            let Some(out) = out.downcast_ref::<T>() else {
                panic_expected_different_type::<&T>()
            };
            storage.store(out)
        }
    }
}

impl<S, H> Clone for Ops<S, H>
where
    S: Storage,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, H> Copy for Ops<S, H> where S: Storage {}
