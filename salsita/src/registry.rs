use crate::INCONSISTENT_STATE;
use crate::Snapshot;
use crate::intern::InternId;
use crate::intern::Interner;
use crate::metrics::Metrics;
use crate::panic_expected_different_type;
use crate::query::Query;
use core::any::Any;
use core::any::TypeId;
use core::hash::Hash;
use std::collections::HashMap;
use std::sync::RwLock;

#[derive(Debug, Default)]
pub(crate) struct QueryRegistry<M> {
    ops: RwLock<HashMap<TypeId, Ops<M>>>,
}

#[derive(Debug)]
pub(crate) struct Ops<M> {
    pub(crate) eval:
        fn(snapshot: &Snapshot<M>, args: &(dyn Any + Send + Sync)) -> Box<dyn Any + Send + Sync>,
    pub(crate) intern_output: fn(interner: &Interner, value: &(dyn Any + Send + Sync)) -> InternId,
}

impl<M> QueryRegistry<M>
where
    M: Metrics,
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

    pub(crate) fn get(&self, query_id: TypeId) -> Option<Ops<M>> {
        self.ops
            .read()
            .expect(INCONSISTENT_STATE)
            .get(&query_id)
            .copied()
    }
}

impl<M> Ops<M>
where
    M: Metrics,
{
    fn new<Q>() -> Self
    where
        Q: Query,
    {
        return Self {
            eval: eval::<M, Q>,
            intern_output: intern_output::<Q::Out>,
        };

        fn eval<M, Q>(
            snapshot: &Snapshot<M>,
            args: &(dyn Any + Send + Sync),
        ) -> Box<dyn Any + Send + Sync>
        where
            M: Metrics,
            Q: Query,
        {
            let Some(args) = args.downcast_ref::<Q::Args>() else {
                panic_expected_different_type::<&Q::Args>()
            };
            let out = Q::eval(snapshot, args);
            Box::new(out)
        }

        fn intern_output<T>(interner: &Interner, out: &(dyn Any + Send + Sync)) -> InternId
        where
            T: Clone + Eq + Hash + Send + Sync + 'static,
        {
            let Some(out) = out.downcast_ref::<T>() else {
                panic_expected_different_type::<&T>()
            };
            interner.intern(out)
        }
    }
}

impl<M> Clone for Ops<M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M> Copy for Ops<M> {}
