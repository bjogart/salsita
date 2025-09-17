use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use salsita::Db;
use salsita::Query;
use salsita::Sig;

// TODO remove main.rs once debugging is no longer necessary
fn main() {
    static EVALS: AtomicUsize = AtomicUsize::new(0);

    struct BurritoPrice;
    impl Sig for BurritoPrice {
        type Args = ();
        type Output = usize;
    }
    impl Query for BurritoPrice {
        fn eval(_: &Db, (): &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            8
        }
    }

    struct BurritoPriceWithShipping;
    impl Sig for BurritoPriceWithShipping {
        type Args = ();
        type Output = usize;
    }
    impl Query for BurritoPriceWithShipping {
        fn eval(db: &Db, (): &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            db.query::<BurritoPrice>(()) + 2
        }
    }

    struct NumBurritos;
    impl Sig for NumBurritos {
        type Args = ();
        type Output = usize;
    }
    impl Query for NumBurritos {
        fn eval(_: &Db, (): &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            3
        }
    }

    struct TotalPrice;
    impl Sig for TotalPrice {
        type Args = ();
        type Output = usize;
    }
    impl Query for TotalPrice {
        fn eval(db: &Db, (): &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            db.query::<BurritoPriceWithShipping>(()) * db.query::<NumBurritos>(())
        }
    }

    struct SalsaPerBurrito;
    impl Sig for SalsaPerBurrito {
        type Args = ();
        type Output = usize;
    }
    impl Query for SalsaPerBurrito {
        fn eval(_: &Db, (): &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            40
        }
    }

    struct SalsaInOrder;
    impl Sig for SalsaInOrder {
        type Args = ();
        type Output = usize;
    }
    impl Query for SalsaInOrder {
        fn eval(db: &Db, (): &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            db.query::<NumBurritos>(()) * db.query::<SalsaPerBurrito>(())
        }
    }

    let db = Db::default();
    assert_eq!(EVALS.load(Ordering::SeqCst), 0);
    let burrito_price = db.query::<BurritoPrice>(());
    assert_eq!((EVALS.load(Ordering::SeqCst), burrito_price), (1, 8));
    let price_with_shipping = db.query::<BurritoPriceWithShipping>(());
    assert_eq!((EVALS.load(Ordering::SeqCst), price_with_shipping), (3, 10));
    let num_burritos = db.query::<NumBurritos>(());
    assert_eq!((EVALS.load(Ordering::SeqCst), num_burritos), (4, 3));
    let total_price = db.query::<TotalPrice>(());
    assert_eq!((EVALS.load(Ordering::SeqCst), total_price), (8, 30));
    let salsa_per_burrito = db.query::<SalsaPerBurrito>(());
    assert_eq!((EVALS.load(Ordering::SeqCst), salsa_per_burrito), (9, 40));
    let salsa_in_order = db.query::<SalsaInOrder>(());
    assert_eq!((EVALS.load(Ordering::SeqCst), salsa_in_order), (12, 120));
    let total_price = db.query::<TotalPrice>(());
    assert_eq!((EVALS.load(Ordering::SeqCst), total_price), (16, 30));
}
