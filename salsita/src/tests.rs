use crate::Db;
use crate::intern::InputId;
use crate::metrics::Metrics;
use crate::metrics::PerfMetrics;
use crate::query::Input;
use crate::query::Query;
use core::fmt;

#[test]
fn db_starts_empty() {
    assert_eq!(metrics_snapshot(&Db::default()), (0, 0, 0));
}

#[test]
fn queries_are_memoized_after_first_call() {
    let mut db = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    assert_queries(
        &db,
        price,
        count,
        burrito_salsa,
        (10, 4, 2, 1),
        (30, 5, 4, 2),
        (35, 6, 5, 3),
        (120, 7, 3, 1),
    );
}

#[test]
fn only_dependent_queries_recompute_on_input_change() {
    let mut db = Db::default();
    let (_, count, burrito_salsa) = init_inputs(&mut db);
    let discount_price = db.new_input::<BurritoPrice>(4);
    assert_queries(
        &db,
        discount_price,
        count,
        burrito_salsa,
        (6, 5, 2, 1),
        (18, 6, 4, 2),
        (23, 7, 5, 3),
        (120, 8, 3, 1),
    );
}

#[test]
fn unchanged_outputs_stop_propagation() {
    let mut db = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    db.set_input(price, 4);
    db.set_input(count, 5);
    assert_queries(
        &db,
        price,
        count,
        burrito_salsa,
        (6, 4, 2, 1),
        (30, 5, 4, 2),
        (35, 6, 5, 3),
        (200, 7, 3, 1),
    );
}

#[test]
fn propagation_updates_transitive_dependents() {
    let mut db = Db::default();
    let (price, count, _) = init_inputs(&mut db);
    assert_query_delta::<PriceWithVat>(&db, &(price, count), 35, 6, 5, 3);
    db.set_input(price, 4);
    assert_query_delta::<PriceWithVat>(&db, &(price, count), 23, 6, 5, 3);
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

fn assert_queries(
    db: &Db<PerfMetrics>,
    price: InputId<BurritoPrice>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
    price_w_shipping: (usize, usize, usize, usize),
    total_price: (usize, usize, usize, usize),
    price_with_vat: (usize, usize, usize, usize),
    salsa_in_order: (usize, usize, usize, usize),
) {
    assert_query_delta::<BurritoPriceWithShipping>(
        db,
        &price,
        price_w_shipping.0,
        price_w_shipping.1,
        price_w_shipping.2,
        price_w_shipping.3,
    );
    assert_query_delta::<TotalPrice>(
        db,
        &(price, count),
        total_price.0,
        total_price.1,
        total_price.2,
        total_price.3,
    );
    assert_query_delta::<PriceWithVat>(
        db,
        &(price, count),
        price_with_vat.0,
        price_with_vat.1,
        price_with_vat.2,
        price_with_vat.3,
    );
    assert_query_delta::<SalsaInOrder>(
        db,
        &(burrito_salsa, count),
        salsa_in_order.0,
        salsa_in_order.1,
        salsa_in_order.2,
        salsa_in_order.3,
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

fn assert_query_delta<Q>(
    db: &Db<PerfMetrics>,
    args: &Q::Args,
    exp_out: Q::Out,
    memo_count: usize,
    dq: usize,
    de: usize,
) where
    Q: Query,
    Q::Out: Eq + fmt::Debug,
{
    let (_, q_before, e_before) = metrics_snapshot(db);
    let out = db.query::<Q>(args);
    assert_eq!(out, exp_out);
    let (memo_count_after, q_after, e_after) = metrics_snapshot(db);
    assert_eq!(
        (memo_count_after, q_after - q_before, e_after - e_before),
        (memo_count, dq, de)
    );
}

fn metrics_snapshot(db: &Db<PerfMetrics>) -> (usize, usize, usize) {
    let m = db.metrics();
    (m.memo_count(), m.query_count(), m.eval_count())
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
