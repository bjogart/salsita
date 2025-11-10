use crate::Db;
use crate::Snapshot;
use crate::event;
use crate::event::PerfHandler;
use crate::query::Input;
use crate::query::InputId;
use crate::query::Query;
use crate::storage::DefaultStorage;
use crate::storage::Storage;
use core::fmt::Debug;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

#[test]
fn db_starts_empty() {
    assert_eq!(
        counts_snapshot(&Db::<DefaultStorage, PerfHandler>::default().snapshot()),
        (0, 0,)
    );
}

#[test]
fn queries_are_memoized_after_first_call() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
}

#[test]
fn only_dependent_queries_recompute_on_input_change() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    let discount_price = db.new_input::<BurritoPrice>(&4);
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
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    db.set_input(price, &4);
    db.set_input(count, &5);
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
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    assert_query_delta::<PriceWithVat>(&db, &(price, count), &Some(35), 1, 0);
    db.set_input(price, &4);
    assert_query_delta::<PriceWithVat>(&db, &(price, count), &Some(23), 5, 3);
}

#[test]
fn modifications_are_blocked_until_snapshots_drop() {
    let mut db: Db<DefaultStorage, ()> = Db::default();
    let (sender, receiver) = mpsc::channel::<Snapshot>();
    let price = db.new_input::<BurritoPrice>(&8);
    // Sanity check: on the main thread we observe the value we just created.
    assert_eq!(*db.snapshot().query::<BurritoPrice>(&price), 8);
    let handle = thread::spawn({
        // Create one snapshot, which moves to the other thread, where it will
        // remain live until we drop it.
        let snapshot = db.snapshot();
        move || {
            {
                let snapshot = snapshot;
                // While this snapshot is live, `set_input` blocks &and will not
                // modify the input.
                assert_eq!(*snapshot.query::<BurritoPrice>(&price), 8);
                // Snapshot is dropped here; meaning that `set_input` is &free to
                // modify its input.
            }
            let snapshot = receiver.recv().unwrap();
            // The new snapshot sees the updated input.
            assert_eq!(*snapshot.query::<BurritoPrice>(&price), 4);
        }
    });
    db.set_input(price, &4);
    sender.send(db.snapshot()).unwrap();
    handle.join().unwrap();
}

#[test]
fn modifications_trigger_query_cancellation() {
    let mut db: Db<DefaultStorage, ()> = Db::default();
    let worker_ready = Arc::new((Mutex::new(false), Condvar::new()));
    let price = db.new_input::<BurritoPrice>(&8);
    // Spawn a worker thread that holds a live snapshot. While this snapshot
    // exists, calling `set_input()` &in the main thread should block and trigger
    // cancellation inside the worker thread.
    let handle = thread::spawn({
        let snapshot = db.snapshot();
        let worker_ready = worker_ready.clone();
        move || {
            // Sanity check: before any cancellation, the query evaluates as
            // expected.
            assert_eq!(
                *snapshot.query::<BurritoPriceWithShipping>(&price),
                Some(10)
            );
            // Notify the main thread that the worker is ready.
            let (mutex, cvar) = &*worker_ready;
            *mutex.lock().unwrap() = true;
            cvar.notify_one();
            // Wait for cancellation to be signalled, which is the signal that
            // `set_input` is &blocking.
            while !snapshot.should_cancel() {
                thread::yield_now();
            }
            // Calling the same query will immediately return the memoized
            // value.
            assert_eq!(
                *snapshot.query::<BurritoPriceWithShipping>(&price),
                Some(10)
            );
        }
    });
    // Wait until the worker is ready.
    let (mutex, cvar) = &*worker_ready;
    let mut guard = mutex.lock().unwrap();
    while !*guard {
        guard = cvar.wait(guard).unwrap();
    }
    // With the worker snapshot still alive, this call will: set the global
    // cancellation flag and block until the worker snapshot is dropped.
    db.set_input(price, &4);
    // Now that the new value for `price` is set successfully, calling the same
    // query will return an updated value.
    assert_eq!(
        *db.snapshot().query::<BurritoPriceWithShipping>(&price),
        Some(6)
    );
    // Join and unwrap the worker thread to propagate failed assertions.
    handle.join().unwrap();
}

fn init_queries(
    db: &Db<DefaultStorage, PerfHandler>,
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
    db: &mut Db<DefaultStorage, PerfHandler>,
) -> (
    InputId<BurritoPrice>,
    InputId<BurritoCount>,
    InputId<SalsaPerBurrito>,
) {
    let price = db.new_input::<BurritoPrice>(&8);
    let count = db.new_input::<BurritoCount>(&3);
    let burrito_salsa = db.new_input::<SalsaPerBurrito>(&40);
    (price, count, burrito_salsa)
}

fn assert_queries(
    db: &Db<DefaultStorage, PerfHandler>,
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
        &Some(price_w_shipping.0),
        price_w_shipping.1,
        price_w_shipping.2,
    );
    assert_query_delta::<TotalPrice>(
        db,
        &(price, count),
        &Some(total_price.0),
        total_price.1,
        total_price.2,
    );
    assert_query_delta::<PriceWithVat>(
        db,
        &(price, count),
        &Some(price_with_vat.0),
        price_with_vat.1,
        price_with_vat.2,
    );
    assert_query_delta::<SalsaInOrder>(
        db,
        &(burrito_salsa, count),
        &Some(salsa_in_order.0),
        salsa_in_order.1,
        salsa_in_order.2,
    );
}

fn assert_query_delta<Q>(
    db: &Db<DefaultStorage, PerfHandler>,
    args: &Q::Args,
    exp_out: &Q::Out,
    dq: usize,
    de: usize,
) where
    Q: Query,
    Q::Out: Eq + Debug,
{
    let snapshot = db.snapshot();
    let (q_before, e_before) = counts_snapshot(&snapshot);
    let out = snapshot.query::<Q>(args);
    assert_eq!(out.as_ref(), exp_out);
    let (q_after, e_after) = counts_snapshot(&snapshot);
    assert_eq!((q_after - q_before, e_after - e_before), (dq, de));
}

fn counts_snapshot(snapshot: &Snapshot<DefaultStorage, PerfHandler>) -> (usize, usize) {
    let m = snapshot.event_handler();
    (m.query_count(), m.eval_count())
}

struct BurritoPrice;
impl Input for BurritoPrice {
    type Value = usize;
}

struct BurritoPriceWithShipping;
impl Query for BurritoPriceWithShipping {
    type Args = InputId<BurritoPrice>;
    type Out = Option<usize>;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        Some(*snapshot.query::<BurritoPrice>(args) + 2)
    }
}

struct BurritoCount;
impl Input for BurritoCount {
    type Value = usize;
}

struct TotalPrice;
impl Query for TotalPrice {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = Option<usize>;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (price, count) = args;
        Some(
            (*snapshot.query::<BurritoPriceWithShipping>(price))?
                * *snapshot.query::<BurritoCount>(count),
        )
    }
}

struct PriceWithVat;
impl Query for PriceWithVat {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = Option<usize>;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        Some((*snapshot.query::<TotalPrice>(args))? + 5)
    }
}

struct SalsaPerBurrito;
impl Input for SalsaPerBurrito {
    type Value = usize;
}

struct SalsaInOrder;
impl Query for SalsaInOrder {
    type Args = (InputId<SalsaPerBurrito>, InputId<BurritoCount>);
    type Out = Option<usize>;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (burrito_salsa, count) = args;
        Some(
            *snapshot.query::<BurritoCount>(count)
                * *snapshot.query::<SalsaPerBurrito>(burrito_salsa),
        )
    }
}
