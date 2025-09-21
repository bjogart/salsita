use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use salsita::Db;
use salsita::Input;
use salsita::InputId;
use salsita::Query;
use salsita::Sig;

// TODO move to tests and remove main.rs once debugging is no longer necessary
fn main() {
    static EVALS: AtomicUsize = AtomicUsize::new(0);

    struct BurritoPrice;
    impl Input for BurritoPrice {
        type Value = usize;
    }

    struct BurritoPriceWithShipping;
    impl Sig for BurritoPriceWithShipping {
        type Args = InputId<BurritoPrice>;
        type Output = usize;
    }
    impl Query for BurritoPriceWithShipping {
        fn eval(db: &Db, burrito_price: &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            db.query::<BurritoPrice>(burrito_price) + 2
        }
    }

    struct NumBurritos;
    impl Input for NumBurritos {
        type Value = usize;
    }

    struct TotalPrice;
    impl Sig for TotalPrice {
        type Args = (InputId<BurritoPrice>, InputId<NumBurritos>);
        type Output = usize;
    }
    impl Query for TotalPrice {
        fn eval(db: &Db, args: &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            let (burrito_price, num_burritos) = args;
            db.query::<BurritoPriceWithShipping>(burrito_price)
                * db.query::<NumBurritos>(num_burritos)
        }
    }

    struct SalsaPerBurrito;
    impl Input for SalsaPerBurrito {
        type Value = usize;
    }

    struct SalsaInOrder;
    impl Sig for SalsaInOrder {
        type Args = (InputId<SalsaPerBurrito>, InputId<NumBurritos>);
        type Output = usize;
    }
    impl Query for SalsaInOrder {
        fn eval(db: &Db, args: &Self::Args) -> Self::Output {
            EVALS.fetch_add(1, Ordering::AcqRel);
            let (salsa_per_burrito, num_burritos) = args;
            db.query::<NumBurritos>(num_burritos) * db.query::<SalsaPerBurrito>(salsa_per_burrito)
        }
    }

    let mut db = Db::default();
    assert_eq!(EVALS.load(Ordering::Acquire), 0);
    let burrito_price = db.new_input::<BurritoPrice>(8);
    let price_with_shipping = db.query::<BurritoPriceWithShipping>(&burrito_price);
    assert_eq!(
        (EVALS.load(Ordering::Acquire), price_with_shipping),
        (1, 10)
    );
    let num_burritos = db.new_input::<NumBurritos>(3);
    let total_price = db.query::<TotalPrice>(&(burrito_price, num_burritos));
    assert_eq!((EVALS.load(Ordering::Acquire), total_price), (2, 30));
    let salsa_per_burrito = db.new_input::<SalsaPerBurrito>(40);
    let salsa_in_order = db.query::<SalsaInOrder>(&(salsa_per_burrito, num_burritos));
    assert_eq!((EVALS.load(Ordering::Acquire), salsa_in_order), (3, 120));
    let total_price = db.query::<TotalPrice>(&(burrito_price, num_burritos));
    assert_eq!((EVALS.load(Ordering::Acquire), total_price), (3, 30));
}
