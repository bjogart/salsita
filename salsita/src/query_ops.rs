use crate::INCONSISTENT_STATE;
use crate::Snapshot;
use crate::event;
use crate::event::Event;
use crate::event::EventKind;
use crate::panic_expected_different_type;
use crate::query::Query;
use crate::storage::Handle as _;
use crate::storage::Storage;
use core::any::Any;
use core::any::TypeId;
use core::hash::Hash;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Default)]
pub(crate) struct QueryOpsRegistry<S, H>(RwLock<QueryOpsRegistryInner<S, H>>)
where
    S: Storage;

#[derive(Debug, Default)]
struct QueryOpsRegistryInner<S, H>(HashMap<TypeId, QueryOps<S, H>>)
where
    S: Storage;

#[derive(Debug)]
pub(crate) struct QueryOps<S, H>
where
    S: Storage,
{
    pub(crate) eval: Eval<S, H>,
    pub(crate) store_output: StoreOut<S, H>,
}

type Eval<S, H> =
    fn(snapshot: &Snapshot<S, H>, args: <S as Storage>::Handle) -> Box<dyn Any + Send + Sync>;

type StoreOut<S, H> =
    fn(storage: &S, handler: &H, value: &(dyn Any + Send + Sync)) -> <S as Storage>::Id;

impl<S, H> QueryOpsRegistry<S, H>
where
    S: Storage,
    H: event::Handler,
{
    pub(crate) fn query_id<Q>(&self, handler: &H) -> TypeId
    where
        Q: Query,
    {
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .query_id::<Q>(handler)
    }

    pub(crate) fn get(&self, query_id: TypeId) -> Option<QueryOps<S, H>> {
        self.0.read().expect(INCONSISTENT_STATE).get(query_id)
    }
}

impl<S, H> QueryOpsRegistryInner<S, H>
where
    S: Storage,
    H: event::Handler,
{
    fn query_id<Q>(&mut self, handler: &H) -> TypeId
    where
        Q: Query,
    {
        let query_id = TypeId::of::<Q>();
        self.0.entry(query_id).or_insert_with(|| {
            handler.event(Event::new(EventKind::RegisterQueryOps));
            QueryOps::new::<Q>()
        });
        query_id
    }

    fn get(&self, query_id: TypeId) -> Option<QueryOps<S, H>> {
        self.0.get(&query_id).copied()
    }
}

impl<S, H> QueryOps<S, H>
where
    S: Storage,
    H: event::Handler,
{
    fn new<Q>() -> Self
    where
        Q: Query,
    {
        return Self {
            eval: eval::<S, H, Q>,
            store_output: store_output::<S, H, Q::Out>,
        };

        fn eval<S, H, Q>(snapshot: &Snapshot<S, H>, args: S::Handle) -> Box<dyn Any + Send + Sync>
        where
            S: Storage,
            H: event::Handler,
            Q: Query,
        {
            let args = args.downcast::<Q::Args>();
            let out = Q::eval(snapshot, &args);
            Box::new(out)
        }

        fn store_output<S, H, T>(storage: &S, handler: &H, out: &(dyn Any + Send + Sync)) -> S::Id
        where
            S: Storage,
            H: event::Handler,
            T: Clone + Eq + Hash + Send + Sync + 'static,
        {
            let Some(out) = out.downcast_ref::<T>() else {
                panic_expected_different_type::<&T>()
            };
            storage.store(handler, out)
        }
    }
}

impl<S, H> Clone for QueryOps<S, H>
where
    S: Storage,
{
    fn clone(&self) -> Self {
        *self
    }
}

impl<S, H> Copy for QueryOps<S, H> where S: Storage {}
