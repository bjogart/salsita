use core::any::Any;
use core::any::TypeId;
use core::cell::Ref;
use core::cell::RefCell;
use core::cell::RefMut;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::hash::Hash;

#[derive(Default)]
pub struct Db {
    registry: RefCell<HashMap<TypeId, QueryId>>,
    queries: Queries,
}

#[derive(Clone, Copy)]
struct QueryId {
    idx: usize,
}

#[derive(Default)]
struct Queries(RefCell<Vec<QueryData>>);

struct QueryData {
    memos: Box<dyn Any>,
}

#[allow(type_alias_bounds)]
type Memos<S>
where
    S: Sig,
= HashMap<S::Args, S::Output>;

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
        let id = self.query_id::<Q>();
        self.ensure_memoized::<Q>(id, &args);
        self.memoized::<Q>(id, args)
    }

    fn ensure_memoized<Q>(&self, id: QueryId, args: &Q::Args)
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
    {
        if !self.queries.query(id).memos::<Q>().contains_key(args) {
            let output = Q::eval(self, args);
            self.queries
                .query_mut(id)
                .memos_mut::<Q>()
                .insert(args.clone(), output);
        }
    }

    fn memoized<Q>(&self, id: QueryId, args: Q::Args) -> Q::Output
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
        Q::Output: Clone,
    {
        self.queries
            .query(id)
            .memos::<Q>()
            .get(&args)
            .unwrap()
            .clone()
    }

    fn query_id<S>(&self) -> QueryId
    where
        S: Sig,
    {
        match self.registry.borrow_mut().entry(TypeId::of::<S>()) {
            Entry::Vacant(entry) => *entry.insert(self.queries.register::<S>()),
            Entry::Occupied(entry) => *entry.get(),
        }
    }
}

impl Queries {
    fn query(&self, query: QueryId) -> Ref<'_, QueryData> {
        Ref::map(self.0.borrow(), |queries| queries.get(query.idx).unwrap())
    }

    fn query_mut(&self, query: QueryId) -> RefMut<'_, QueryData> {
        RefMut::map(self.0.borrow_mut(), |queries| {
            queries.get_mut(query.idx).unwrap()
        })
    }

    fn register<S>(&self) -> QueryId
    where
        S: Sig,
    {
        let mut this = self.0.borrow_mut();
        let id = QueryId { idx: this.len() };
        this.push(QueryData::new::<S>());
        id
    }
}

impl QueryData {
    fn new<S>() -> Self
    where
        S: Sig,
    {
        Self {
            memos: Box::new(Memos::<S>::default()),
        }
    }

    fn memos<S>(&self) -> &Memos<S>
    where
        S: Sig,
    {
        self.memos.downcast_ref().unwrap()
    }

    fn memos_mut<S>(&mut self) -> &mut Memos<S>
    where
        S: Sig,
    {
        self.memos.downcast_mut().unwrap()
    }
}
