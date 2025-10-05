use core::fmt;
use salsita::Db;
use salsita::Input;
use salsita::Query;
use salsita::intern::InputId;
use salsita::metrics::Metrics;
use salsita::metrics::PerfMetrics;

// TODO move to tests and remove main.rs once debugging is no longer necessary
fn main() {
    struct BurritoPrice;
    impl Input for BurritoPrice {
        type Value = usize;
    }

    struct BurritoPriceWithShipping;
    impl Query for BurritoPriceWithShipping {
        type Args = InputId<BurritoPrice>;
        type Out = usize;

        fn eval<M>(db: &Db<M>, args: &Self::Args) -> Self::Out
        where
            M: Metrics,
        {
            db.query::<BurritoPrice>(args) + 2
        }
    }

    struct BurritoCount;
    impl Input for BurritoCount {
        type Value = usize;
    }

    struct TotalPrice;
    impl Query for TotalPrice {
        type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
        type Out = usize;

        fn eval<M>(db: &Db<M>, args: &Self::Args) -> Self::Out
        where
            M: Metrics,
        {
            let (price, count) = args;
            db.query::<BurritoPriceWithShipping>(price) * db.query::<BurritoCount>(count)
        }
    }

    struct SalsaPerBurrito;
    impl Input for SalsaPerBurrito {
        type Value = usize;
    }

    struct SalsaInOrder;
    impl Query for SalsaInOrder {
        type Args = (InputId<SalsaPerBurrito>, InputId<BurritoCount>);
        type Out = usize;

        fn eval<M>(db: &Db<M>, args: &Self::Args) -> Self::Out
        where
            M: Metrics,
        {
            let (salsa_per, count) = args;
            db.query::<BurritoCount>(count) * db.query::<SalsaPerBurrito>(salsa_per)
        }
    }

    let mut db = Db::<PerfMetrics>::default();
    {
        let m = db.metrics();
        assert_eq!((m.query_count(), m.eval_count()), (0, 0));
    }

    let price = db.new_input::<BurritoPrice>(8);
    assert_query::<BurritoPriceWithShipping>(&mut db, &price, 10, 2, 1);
    let count = db.new_input::<BurritoCount>(3);
    assert_query::<TotalPrice>(&mut db, &(price, count), 30, 6, 3);
    let salsa_per = db.new_input::<SalsaPerBurrito>(40);
    assert_query::<SalsaInOrder>(&mut db, &(salsa_per, count), 120, 9, 4);

    let discount_price = db.new_input::<BurritoPrice>(4);
    assert_query::<BurritoPriceWithShipping>(&mut db, &discount_price, 6, 11, 5);
    assert_query::<TotalPrice>(&mut db, &(discount_price, count), 18, 15, 7);
    assert_query::<SalsaInOrder>(&mut db, &(salsa_per, count), 120, 18, 8);

    db.set_input(count, 5);
    assert_query::<BurritoPriceWithShipping>(&mut db, &discount_price, 6, 20, 9);
    assert_query::<TotalPrice>(&mut db, &(discount_price, count), 30, 24, 11);
    assert_query::<SalsaInOrder>(&mut db, &(salsa_per, count), 200, 27, 12);
}

fn assert_query<Q>(
    db: &mut Db<PerfMetrics>,
    args: &Q::Args,
    exp_out: Q::Out,
    query_count: usize,
    eval_count: usize,
) where
    Q: Query,
    Q::Out: Eq + fmt::Debug,
{
    let out = db.query::<Q>(args);
    assert_eq!(out, exp_out);
    let m = db.metrics();
    assert_eq!((m.query_count(), m.eval_count()), (query_count, eval_count));
}
