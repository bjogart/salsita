use core::any::Any;
use core::any::TypeId;
use core::cell::RefCell;
use std::collections::HashMap;
use std::collections::hash_map::Entry;

#[derive(Default)]
pub struct Db {
    registry: RefCell<HashMap<TypeId, EvalId>>,
    evals: RefCell<Vec<fn(&Db, &dyn Any) -> Box<dyn Any>>>,
}

#[derive(Clone, Copy)]
struct EvalId {
    idx: usize,
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
        let eval_id = {
            match self.registry.borrow_mut().entry(TypeId::of::<Q>()) {
                Entry::Vacant(entry) => {
                    let mut evals = self.evals.borrow_mut();
                    let id = EvalId { idx: evals.len() };
                    evals.push(eval::<Q>);
                    *entry.insert(id)
                }
                Entry::Occupied(entry) => *entry.get(),
            }
        };
        let eval = *self.evals.borrow().get(eval_id.idx).unwrap();
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
