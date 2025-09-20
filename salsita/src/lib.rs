use core::any::Any;
use core::any::TypeId;
use core::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;

#[derive(Default)]
pub struct Db {
    registry: RefCell<HashMap<TypeId, QueryId>>,
    queries: RefCell<Vec<QueryData>>,
}

#[derive(Clone, Copy)]
struct QueryId {
    idx: usize,
}

struct QueryData {
    cache: Box<dyn Any>,
}

pub trait Query: Sig {
    fn eval(db: &Db, args: &Self::Args) -> Self::Output;
}

pub trait Sig: 'static {
    type Args;
    type Output;
}

impl Db {
    pub fn query<Q>(&self, args: Q::Args) -> Q::Output
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
        Q::Output: Clone,
    {
        let query = self.query_id::<Q>();
        let is_cached = {
            let queries = self.queries.borrow();
            let query = queries.get(query.idx).unwrap();
            let cache: &HashMap<Q::Args, Q::Output> = query.cache.downcast_ref().unwrap();
            cache.contains_key(&args)
        };
        if !is_cached {
            let output = Q::eval(self, &args);
            let mut queries = self.queries.borrow_mut();
            let query = queries.get_mut(query.idx).unwrap();
            let cache: &mut HashMap<Q::Args, Q::Output> = query.cache.downcast_mut().unwrap();
            cache.insert(args.clone(), output);
        }
        let queries = self.queries.borrow();
        let query = queries.get(query.idx).unwrap();
        let cache = query
            .cache
            .downcast_ref::<HashMap<Q::Args, Q::Output>>()
            .unwrap();
        cache.get(&args).unwrap().clone()
    }

    fn query_id<Q>(&self) -> QueryId
    where
        Q: Query,
    {
        let query = {
            match self.registry.borrow_mut().entry(TypeId::of::<Q>()) {
                Entry::Vacant(entry) => {
                    let mut data = self.queries.borrow_mut();
                    let id = QueryId { idx: data.len() };
                    data.push(QueryData::new::<Q>());
                    *entry.insert(id)
                }
                Entry::Occupied(entry) => *entry.get(),
            }
        };
        query
    }
}

impl QueryData {
    fn new<Q>() -> Self
    where
        Q: Query,
    {
        Self {
            cache: Box::new(HashMap::<Q::Args, Q::Output>::default()),
        }
    }
}
