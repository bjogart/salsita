use core::fmt;
use salsita::Db;
use salsita::Input;
use salsita::Query;
use salsita::intern::InputId;
use salsita::metrics::Metrics;
use salsita::metrics::PerfMetrics;

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

struct PriceWithVat;
impl Query for PriceWithVat {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<M>(db: &Db<M>, args: &Self::Args) -> Self::Out
    where
        M: Metrics,
    {
        db.query::<TotalPrice>(args) + 5
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

fn assert_query<Q>(
    db: &mut Db<PerfMetrics>,
    args: &Q::Args,
    exp_out: &Q::Out,
    query_count: usize,
    eval_count: usize,
) where
    Q: Query,
    Q::Out: Eq + fmt::Debug,
{
    let out = db.query::<Q>(args);
    assert_eq!(out, *exp_out);
    let m = db.metrics();
    assert_eq!((m.query_count(), m.eval_count()), (query_count, eval_count));
}

fn main() {
    let mut db = Db::<PerfMetrics>::default();
    {
        let m = db.metrics();
        assert_eq!((m.query_count(), m.eval_count()), (0, 0))
    };

    let price = db.new_input::<BurritoPrice>(8);
    assert_query::<BurritoPriceWithShipping>(&mut db, &price, &10, 2, 1);
    let count = db.new_input::<BurritoCount>(3);
    assert_query::<TotalPrice>(&mut db, &(price, count), &30, 6, 3);
    assert_query::<PriceWithVat>(&mut db, &(price, count), &35, 11, 6);
    let salsa_per = db.new_input::<SalsaPerBurrito>(40);
    assert_query::<SalsaInOrder>(&mut db, &(salsa_per, count), &120, 14, 7);

    let discount_price = db.new_input::<BurritoPrice>(4);
    assert_query::<BurritoPriceWithShipping>(&mut db, &discount_price, &6, 16, 8);
    assert_query::<TotalPrice>(&mut db, &(discount_price, count), &18, 20, 10);
    assert_query::<PriceWithVat>(&mut db, &(discount_price, count), &23, 25, 13);
    assert_query::<SalsaInOrder>(&mut db, &(salsa_per, count), &120, 28, 14);

    db.set_input(price, 4);
    db.set_input(count, 5);
    assert_query::<BurritoPriceWithShipping>(&mut db, &price, &6, 30, 15);
    assert_query::<TotalPrice>(&mut db, &(price, count), &30, 34, 17);
    assert_query::<PriceWithVat>(&mut db, &(price, count), &35, 39, 20);
    assert_query::<SalsaInOrder>(&mut db, &(salsa_per, count), &200, 42, 21);
}
