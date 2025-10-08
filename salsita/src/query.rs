use crate::Db;
use crate::intern::InputId;
use crate::metrics::Metrics;
use core::hash::Hash;

pub trait Query: 'static {
    type Args: Clone + Eq + Hash;
    type Out: Clone;

    fn eval<M>(db: &Db<M>, args: &Self::Args) -> Self::Out
    where
        M: Metrics;
}

pub trait Input: 'static {
    type Value: Clone;
}

impl<I> Query for I
where
    I: Input,
{
    type Args = InputId<Self>;

    type Out = <Self as Input>::Value;

    fn eval<M>(_: &Db<M>, _: &Self::Args) -> Self::Out
    where
        M: Metrics,
    {
        panic!("Inputs should be defined through `Db::{{new,set}}_input()`, not evaluated")
    }
}
