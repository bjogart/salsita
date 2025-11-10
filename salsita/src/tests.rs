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
use core::ops::Sub;
use std::sync::Arc;
use std::sync::Condvar;
use std::sync::Mutex;
use std::sync::mpsc;
use std::thread;

#[derive(PartialEq, Eq, Debug)]
struct Counts {
    query_count: usize,
    eval_count: usize,
    registered_query_ops: usize,
    stored_values: usize,
    memo_count: usize,
}

#[test]
fn db_starts_empty() {
    assert_eq!(
        Counts::new(&Db::<DefaultStorage, PerfHandler>::default().snapshot()),
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 0,
            memo_count: 0
        }
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
    let discount_price = assert_new_input::<BurritoPrice>(
        &mut db,
        4,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 2,
            memo_count: 1,
        },
    );
    assert_queries(
        &db,
        discount_price,
        count,
        burrito_salsa,
        (
            6,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 1,
                memo_count: 1,
            },
        ),
        (
            18,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 2,
                memo_count: 1,
            },
        ),
        (
            23,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 1,
                memo_count: 1,
            },
        ),
        (
            120,
            Counts {
                query_count: 1,
                eval_count: 0,
                registered_query_ops: 0,
                stored_values: 0,
                memo_count: 0,
            },
        ),
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
        (
            6,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 1,
                memo_count: 0,
            },
        ),
        (
            30,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 0,
                memo_count: 0,
            },
        ),
        (
            35,
            Counts {
                query_count: 1,
                eval_count: 0,
                registered_query_ops: 0,
                stored_values: 0,
                memo_count: 0,
            },
        ),
        (
            200,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 0,
                stored_values: 1,
                memo_count: 0,
            },
        ),
    );
}

#[test]
fn propagation_updates_transitive_dependents() {
    let mut db: Db<DefaultStorage, PerfHandler> = Db::default();
    let (price, count, burrito_salsa) = init_inputs(&mut db);
    init_queries(&db, price, count, burrito_salsa);
    assert_query_delta::<PriceWithVat>(
        &db,
        &(price, count),
        &35,
        Counts {
            query_count: 1,
            eval_count: 0,
            registered_query_ops: 0,
            stored_values: 0,
            memo_count: 0,
        },
    );
    db.set_input(price, &4);
    assert_query_delta::<PriceWithVat>(
        &db,
        &(price, count),
        &23,
        Counts {
            query_count: 5,
            eval_count: 3,
            registered_query_ops: 0,
            stored_values: 3,
            memo_count: 0,
        },
    );
}

#[test]
fn modifications_are_blocked_until_snapshots_drop() {
    let mut db: Db = Db::default();
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
    let mut db: Db = Db::default();
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
            assert_eq!(*snapshot.query::<BurritoPriceWithShipping>(&price), (10));
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
            assert_eq!(*snapshot.query::<BurritoPriceWithShipping>(&price), (10));
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
        (6)
    );
    // Join and unwrap the worker thread to propagate failed assertions.
    handle.join().unwrap();
}

fn init_inputs(
    db: &mut Db<DefaultStorage, PerfHandler>,
) -> (
    InputId<BurritoPrice>,
    InputId<BurritoCount>,
    InputId<SalsaPerBurrito>,
) {
    let price = assert_new_input(
        db,
        8,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 1,
            stored_values: 2,
            memo_count: 1,
        },
    );
    let count = assert_new_input::<BurritoCount>(
        db,
        3,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 1,
            stored_values: 2,
            memo_count: 1,
        },
    );
    let burrito_salsa = assert_new_input::<SalsaPerBurrito>(
        db,
        40,
        Counts {
            query_count: 0,
            eval_count: 0,
            registered_query_ops: 1,
            stored_values: 2,
            memo_count: 1,
        },
    );
    (price, count, burrito_salsa)
}

fn assert_new_input<I>(
    db: &mut Db<DefaultStorage, PerfHandler>,
    value: I::Value,
    counts: Counts,
) -> InputId<I>
where
    I: Input,
{
    let before = Counts::new(&db.snapshot());
    let input_id = db.new_input::<I>(&value);
    let after = Counts::new(&db.snapshot());
    assert_eq!(after - before, counts);
    input_id
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
        (
            10,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 1,
                stored_values: 1,
                memo_count: 1,
            },
        ),
        (
            30,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 1,
                stored_values: 2,
                memo_count: 1,
            },
        ),
        (
            35,
            Counts {
                query_count: 2,
                eval_count: 1,
                registered_query_ops: 1,
                stored_values: 1,
                memo_count: 1,
            },
        ),
        (
            120,
            Counts {
                query_count: 3,
                eval_count: 1,
                registered_query_ops: 1,
                stored_values: 2,
                memo_count: 1,
            },
        ),
    );
}

fn assert_queries(
    db: &Db<DefaultStorage, PerfHandler>,
    price: InputId<BurritoPrice>,
    count: InputId<BurritoCount>,
    burrito_salsa: InputId<SalsaPerBurrito>,
    price_w_shipping: (usize, Counts),
    total_price: (usize, Counts),
    price_with_vat: (usize, Counts),
    salsa_in_order: (usize, Counts),
) {
    assert_query_delta::<BurritoPriceWithShipping>(
        db,
        &price,
        &(price_w_shipping.0),
        price_w_shipping.1,
    );
    assert_query_delta::<TotalPrice>(db, &(price, count), &total_price.0, total_price.1);
    assert_query_delta::<PriceWithVat>(db, &(price, count), &(price_with_vat.0), price_with_vat.1);
    assert_query_delta::<SalsaInOrder>(
        db,
        &(burrito_salsa, count),
        &(salsa_in_order.0),
        salsa_in_order.1,
    );
}

fn assert_query_delta<Q>(
    db: &Db<DefaultStorage, PerfHandler>,
    args: &Q::Args,
    exp_out: &Q::Out,
    d_counts: Counts,
) where
    Q: Query,
    Q::Out: Eq + Debug,
{
    let snapshot = db.snapshot();
    let before = Counts::new(&snapshot);
    let out = snapshot.query::<Q>(args);
    assert_eq!(out.as_ref(), exp_out);
    let after = Counts::new(&snapshot);
    assert_eq!(after - before, d_counts);
}

impl Counts {
    fn new(snapshot: &Snapshot<DefaultStorage, PerfHandler>) -> Self {
        Self {
            query_count: snapshot.event_handler().query_count(),
            eval_count: snapshot.event_handler().eval_count(),
            registered_query_ops: snapshot.event_handler().registered_query_ops(),
            stored_values: snapshot.event_handler().stored_values(),
            memo_count: snapshot.event_handler().memo_count(),
        }
    }
}

impl Sub for Counts {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            query_count: self.query_count - rhs.query_count,
            eval_count: self.eval_count - rhs.eval_count,
            registered_query_ops: self.registered_query_ops - rhs.registered_query_ops,
            stored_values: self.stored_values - rhs.stored_values,
            memo_count: self.memo_count - rhs.memo_count,
        }
    }
}

struct BurritoPrice;
impl Input for BurritoPrice {
    type Value = usize;
}

struct BurritoPriceWithShipping;
impl Query for BurritoPriceWithShipping {
    type Args = InputId<BurritoPrice>;
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        *snapshot.query::<BurritoPrice>(args) + 2
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

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (price, count) = args;
        *snapshot.query::<BurritoPriceWithShipping>(price) * *snapshot.query::<BurritoCount>(count)
    }
}

struct PriceWithVat;
impl Query for PriceWithVat {
    type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
    type Out = usize;

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        (*snapshot.query::<TotalPrice>(args)) + 5
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

    fn eval<S, H>(snapshot: &Snapshot<S, H>, args: &Self::Args) -> Self::Out
    where
        S: Storage,
        H: event::Handler,
    {
        let (burrito_salsa, count) = args;
        *snapshot.query::<BurritoCount>(count) * *snapshot.query::<SalsaPerBurrito>(burrito_salsa)
    }
}
