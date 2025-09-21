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

enum MemoEntry<S>
where
    S: Sig,
{
    InProgress,
    Ready(S::Output),
}

#[allow(type_alias_bounds)]
type Memos<S>
where
    S: Sig,
= HashMap<S::Args, MemoEntry<S>>;

pub trait Query: Sig {
    fn eval(db: &Db, args: &Self::Args) -> Self::Output;
}

pub trait Sig: 'static {
    type Args;
    type Output;
}

impl Db {
    pub fn query<Q>(&self, args: &Q::Args) -> Q::Output
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
        Q::Output: Clone,
    {
        let id = self.query_id::<Q>();
        self.ensure_memoized::<Q>(id, &args);
        self.memoized::<Q>(id, &args)
    }

    fn ensure_memoized<Q>(&self, id: QueryId, args: &Q::Args)
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
    {
        let is_memoized = match self.queries.query_mut(id).memos_mut::<Q>().get(args) {
            Some(MemoEntry::InProgress) => panic!("cycle detected"),
            Some(MemoEntry::Ready(_)) => true,
            None => false,
        };
        if !is_memoized {
            self.queries
                .query_mut(id)
                .memos_mut::<Q>()
                .insert(args.clone(), MemoEntry::InProgress);
            let output = Q::eval(self, args);
            self.queries
                .query_mut(id)
                .memos_mut::<Q>()
                .insert(args.clone(), MemoEntry::Ready(output));
        }
    }

    fn memoized<Q>(&self, id: QueryId, args: &Q::Args) -> Q::Output
    where
        Q: Query,
        Q::Args: Clone + Eq + Hash,
        Q::Output: Clone,
    {
        match self.queries.query(id).memos::<Q>().get(&args) {
            None => panic!("`Db::memoized` called but value is not memoized"),
            Some(MemoEntry::InProgress) => unreachable!(),
            Some(MemoEntry::Ready(output)) => output.clone(),
        }
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

#[cfg(test)]
mod tests {
    use crate::Db;
    use crate::Query;
    use crate::Sig;

    #[test]
    #[should_panic]
    fn cycles_panic() {
        struct Cycle;
        impl Sig for Cycle {
            type Args = ();
            type Output = ();
        }
        impl Query for Cycle {
            fn eval(db: &Db, (): &Self::Args) -> Self::Output {
                db.query::<Self>(&())
            }
        }

        Db::default().query::<Cycle>(&());
    }
}
