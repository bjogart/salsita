use crate::INCONSISTENT_STATE;
use crate::Snapshot;
use crate::event;
use crate::event::Event;
use crate::event::EventKind;
use crate::panic_expected_different_type;
use crate::query::InputId;
use crate::query::Query;
use crate::storage::Handle as _;
use crate::storage::Storage;
use crate::storage::Transfer as _;
use core::any::Any;
use core::any::TypeId;
use core::hash::Hash;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug)]
pub(crate) struct QueryOpsRegistry<S, H>(RwLock<QueryOpsRegistryInner<S, H>>)
where
    S: Storage;

#[derive(Debug)]
pub(crate) struct QueryOpsRegistryInner<S, H>(HashMap<QueryId, QueryOps<S, H>>)
where
    S: Storage;

#[derive(Debug)]
pub(crate) struct QueryOps<S, H>
where
    S: Storage,
{
    pub(crate) eval: Eval<S, H>,
    pub(crate) store_output: StoreOut<S, H>,
    pub(crate) transfer_input_memo_values: TransferInputMemoValues<S>,
}

type Eval<S, H> =
    fn(snapshot: &Snapshot<S, H>, args: <S as Storage>::Handle) -> Box<dyn Any + Send + Sync>;

type StoreOut<S, H> =
    fn(storage: &S, handler: &H, value: &(dyn Any + Send + Sync)) -> <S as Storage>::Id;

type TransferInputMemoValues<S> = fn(
    transfer: &mut <S as Storage>::Transfer,
    curr_args_id: <S as Storage>::Id,
    value_id: <S as Storage>::Id,
) -> (<S as Storage>::Id, <S as Storage>::Id);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) struct QueryId(TypeId);

impl<S, H> QueryOpsRegistry<S, H>
where
    S: Storage,
{
    pub(crate) fn query_id<Q>(&self, handler: &H) -> QueryId
    where
        H: event::Handler,
        Q: Query,
    {
        self.0
            .write()
            .expect(INCONSISTENT_STATE)
            .query_id::<Q>(handler)
    }

    pub(crate) fn get(&self, query_id: QueryId) -> Option<QueryOps<S, H>>
    where
        H: event::Handler,
    {
        self.0.read().expect(INCONSISTENT_STATE).get(query_id)
    }

    pub(crate) fn into_inner(self) -> QueryOpsRegistryInner<S, H> {
        self.0.into_inner().expect(INCONSISTENT_STATE)
    }
}

impl<S, H> Default for QueryOpsRegistry<S, H>
where
    S: Storage,
{
    fn default() -> Self {
        Self(RwLock::default())
    }
}

impl<S, H> QueryOpsRegistryInner<S, H>
where
    S: Storage,
{
    fn query_id<Q>(&mut self, handler: &H) -> QueryId
    where
        H: event::Handler,
        Q: Query,
    {
        let query_id = QueryId(TypeId::of::<Q>());
        self.0.entry(query_id).or_insert_with(|| {
            handler.event(Event::new(EventKind::RegisterQueryOps));
            QueryOps::new::<Q>()
        });
        query_id
    }

    pub(crate) fn get(&self, query_id: QueryId) -> Option<QueryOps<S, H>> {
        self.0.get(&query_id).copied()
    }

    pub(crate) fn remove(&mut self, handler: &H, query_id: QueryId)
    where
        H: event::Handler,
    {
        if let Some(_) = self.0.remove(&query_id) {
            handler.event(Event::new(EventKind::DeregisterQueryOps));
        }
    }

    pub(crate) fn into_registry(mut self) -> QueryOpsRegistry<S, H> {
        self.0.shrink_to_fit();
        QueryOpsRegistry(RwLock::new(self))
    }
}

impl<S, H> Default for QueryOpsRegistryInner<S, H>
where
    S: Storage,
{
    fn default() -> Self {
        Self(HashMap::default())
    }
}

impl<S, H> QueryOps<S, H>
where
    S: Storage,
{
    fn new<Q>() -> Self
    where
        H: event::Handler,
        Q: Query,
    {
        return Self {
            eval: eval::<S, H, Q>,
            store_output: store_output::<S, H, Q::Out>,
            transfer_input_memo_values: transfer_input_memo_values::<S, Q>,
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

        fn transfer_input_memo_values<S, Q>(
            transfer: &mut S::Transfer,
            curr_args_id: S::Id,
            curr_value_id: S::Id,
        ) -> (S::Id, S::Id)
        where
            Q: Query,
            S: Storage,
        {
            let next_args_id = transfer.transfer::<InputId<Q>>(curr_args_id);
            let value_id = transfer.transfer::<Q::Out>(curr_value_id);
            (next_args_id, value_id)
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
