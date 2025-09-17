use core::any::Any;
use core::any::TypeId;
use core::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

#[derive(Default)]
pub struct Db {
    registry: RefCell<HashMap<TypeId, fn(&Db, &dyn Any) -> Box<dyn Any>>>,
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
        Q::Output: Clone,
    {
        let eval = match self.registry.borrow_mut().entry(TypeId::of::<Q>()) {
            Entry::Vacant(entry) => *entry.insert(eval::<Q>),
            Entry::Occupied(entry) => *entry.get(),
        };
        let value = eval(self, &args);
        return value.downcast_ref::<Q::Output>().unwrap().clone();

        fn eval<Q>(db: &Db, args: &dyn Any) -> Box<dyn Any>
        where
            Q: Query,
        {
            let args = args.downcast_ref().unwrap();
            let output = Q::eval(db, args);
            Box::new(output)
        }
    }
}
