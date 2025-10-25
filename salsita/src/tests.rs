use crate::Db;
use crate::Snapshot;
use crate::metrics::Metrics;
use crate::metrics::PerfMetrics;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use core::fmt::Debug;
use std::sync::mpsc::channel;
use std::thread;

#[test]
fn db_starts_empty() {
    assert_eq!(metrics_snapshot(&Db::default().snapshot()), (0, 0,));
}

#[test]
fn queries_are_memoized_after_first_call() {
    let mut db = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
}

#[test]
fn only_dependent_queries_recompute_on_input_change() {
    let mut db = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    let discount_price = db.new_input::<BurritoPrice>(4);
    assert_queries(
        &db,
        discount_price,
        count,
        burrito_salsa,
        (6, 2, 1),
        (18, 3, 1),
        (23, 2, 1),
        (120, 1, 0),
    );
}

#[test]
fn unchanged_outputs_stop_propagation() {
    let mut db = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    db.set_input(price, 4);
    db.set_input(count, 5);
    assert_queries(
        &db,
        price,
        count,
        burrito_salsa,
        (6, 2, 1),
        (30, 3, 1),
        (35, 1, 0),
        (200, 3, 1),
    );
}

#[test]
fn propagation_updates_transitive_dependents() {
    let mut db = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    assert_query_delta::<PriceWithVat>(&db, &(price, count), 35, 1, 0);
    db.set_input(price, 4);
    assert_query_delta::<PriceWithVat>(&db, &(price, count), 23, 5, 3);
}

#[test]
fn modifications_are_blocked_until_snapshots_drop() {
    let mut db: Db<()> = Db::default();
    let (sender, receiver) = channel::<Snapshot<()>>();
    let price = db.new_input::<BurritoPrice>(8);
    assert_eq!(db.snapshot().query::<BurritoPrice>(&price), 8);
    let handle = thread::spawn({
        let snapshot = db.snapshot();
        move || {
            assert_eq!(snapshot.query::<BurritoPrice>(&price), 8);
            std::mem::drop(snapshot);
            let snapshot = receiver.recv().unwrap();
            assert_eq!(snapshot.query::<BurritoPrice>(&price), 4);
        }
    });
    db.set_input(price, 4);
    sender.send(db.snapshot()).unwrap();
    handle.join().unwrap();
}

fn init_queries(
    db: &Db<PerfMetrics>,
    price: InputId<BurritoPrice>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
) {
    assert_queries(
        &db,
        price,
        count,
        burrito_salsa,
        (10, 2, 1),
        (30, 3, 1),
        (35, 2, 1),
        (120, 3, 1),
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
    db: &Db<PerfMetrics>,
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

fn assert_query_delta<Q>(
    db: &Db<PerfMetrics>,
    args: &Q::Args,
    exp_out: Q::Out,
    dq: usize,
    de: usize,
) where
    Q: Query,
    Q::Out: Eq + Debug,
{
    let snapshot = db.snapshot();
    let (q_before, e_before) = metrics_snapshot(&snapshot);
    let out = snapshot.query::<Q>(args);
    assert_eq!(out, exp_out);
    let (q_after, e_after) = metrics_snapshot(&snapshot);
    assert_eq!((q_after - q_before, e_after - e_before), (dq, de));
}

fn metrics_snapshot(snapshot: &Snapshot<PerfMetrics>) -> (usize, usize) {
    let m = snapshot.metrics();
    (m.query_count(), m.eval_count())
}

struct BurritoPrice;
impl Input for BurritoPrice {
    type Value = usize;
}

struct BurritoPriceWithShipping;
impl Query for BurritoPriceWithShipping {
    type Args = InputId<BurritoPrice>;
    type Out = usize;

    fn eval<M>(snapshot: &Snapshot<M>, args: &Self::Args) -> Self::Out
    where
        M: Metrics,
    {
        snapshot.query::<BurritoPrice>(args) + 2
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

    fn eval<M>(snapshot: &Snapshot<M>, args: &Self::Args) -> Self::Out
    where
        M: Metrics,
    {
        let (price, count) = args;
        snapshot.query::<BurritoPriceWithShipping>(price) * snapshot.query::<BurritoCount>(count)
    }
}

struct PriceWithVat;
impl Query for PriceWithVat {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<M>(snapshot: &Snapshot<M>, args: &Self::Args) -> Self::Out
    where
        M: Metrics,
    {
        snapshot.query::<TotalPrice>(args) + 5
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

    fn eval<M>(snapshot: &Snapshot<M>, args: &Self::Args) -> Self::Out
    where
        M: Metrics,
    {
        let (burrito_salsa, count) = args;
        snapshot.query::<BurritoCount>(count) * snapshot.query::<SalsaPerBurrito>(burrito_salsa)
    }
}
