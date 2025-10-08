use core::fmt;
use salsita::Db;
use salsita::Input;
use salsita::Query;
use salsita::intern::InputId;
use salsita::metrics::Metrics;
use salsita::metrics::PerfMetrics;

#[test]
fn db_is_initialized_empty() {
    assert_eq!(metrics_snapshot(&mut empty_db()), (0, 0));
}

#[test]
fn query_outputs_are_memoized() {
    let mut db = empty_db();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    assert_queries(
        &mut db,
        price,
        count,
        burrito_salsa,
        (10, 2, 1),
        (30, 4, 2),
        (35, 5, 3),
        (120, 3, 1),
    );
}

#[test]
fn new_inputs_cause_re_evaluation_only_in_dependent_queries() {
    let mut db = empty_db();
    let (_, count, burrito_salsa) = init_inputs(&mut db);
    let discount_price = db.new_input::<BurritoPrice>(4);
    assert_queries(
        &mut db,
        discount_price,
        count,
        burrito_salsa,
        (6, 2, 1),
        (18, 4, 2),
        (23, 5, 3),
        (120, 3, 1),
    );
}

#[test]
fn change_propagation_stops_if_query_output_remains_the_same() {
    let mut db = empty_db();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    db.set_input(price, 4);
    db.set_input(count, 5);
    assert_queries(
        &mut db,
        price,
        count,
        burrito_salsa,
        (6, 2, 1),
        (30, 4, 2),
        (35, 5, 3),
        (200, 3, 1),
    );
}

fn init_inputs(
    db: &mut Db<PerfMetrics>,
) -> (
    InputId<BurritoPrice>,
    InputId<BurritoCount>,
    InputId<SalsaPerBurrito>,
) {
    let price = db.new_input::<BurritoPrice>(8);
    let count = db.new_input::<BurritoCount>(3);
    let burrito_salsa = db.new_input::<SalsaPerBurrito>(40);
    (price, count, burrito_salsa)
}

fn assert_queries(
    db: &mut Db<PerfMetrics>,
    price: InputId<BurritoPrice>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
    price_w_shipping: (usize, usize, usize),
    total_price: (usize, usize, usize),
    price_with_vat: (usize, usize, usize),
    salsa_in_order: (usize, usize, usize),
) {
    assert_query_delta::<BurritoPriceWithShipping>(
        db,
        &price,
        price_w_shipping.0,
        price_w_shipping.1,
        price_w_shipping.2,
    );
    assert_query_delta::<TotalPrice>(
        db,
        &(price, count),
        total_price.0,
        total_price.1,
        total_price.2,
    );
    assert_query_delta::<PriceWithVat>(
        db,
        &(price, count),
        price_with_vat.0,
        price_with_vat.1,
        price_with_vat.2,
    );
    assert_query_delta::<SalsaInOrder>(
        db,
        &(burrito_salsa, count),
        salsa_in_order.0,
        salsa_in_order.1,
        salsa_in_order.2,
    );
}

#[test]
#[should_panic]
fn cycles_panic() {
    struct Cycle;
    impl Query for Cycle {
        type Args = ();
        type Out = ();

        fn eval<M>(db: &Db<M>, (): &Self::Args) -> Self::Out
        where
            M: Metrics,
        {
            db.query::<Self>(&())
        }
    }

    Db::<()>::default().query::<Cycle>(&());
}

fn empty_db() -> Db<PerfMetrics> {
    Db::default()
}

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
        let (burrito_salsa, count) = args;
        db.query::<BurritoCount>(count) * db.query::<SalsaPerBurrito>(burrito_salsa)
    }
}

fn assert_query_delta<Q>(
    db: &mut Db<PerfMetrics>,
    args: &Q::Args,
    exp_out: Q::Out,
    dq: usize,
    de: usize,
) where
    Q: Query,
    Q::Out: Eq + fmt::Debug,
{
    let before = metrics_snapshot(db);
    let out = db.query::<Q>(args);
    assert_eq!(out, exp_out);
    let after = metrics_snapshot(db);
    assert_eq!((after.0 - before.0, after.1 - before.1), (dq, de));
}

fn metrics_snapshot(db: &mut Db<PerfMetrics>) -> (usize, usize) {
    let m = db.metrics();
    (m.query_count(), m.eval_count())
}
