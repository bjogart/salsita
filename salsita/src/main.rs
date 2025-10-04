use core::sync::atomic::AtomicUsize;
use core::sync::atomic::Ordering;
use salsita::Db;
use salsita::Input;
use salsita::Query;
use salsita::Sig;
use salsita::intern::InputId;

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
        type Out = usize;
    }
    impl Query for BurritoPriceWithShipping {
        fn eval(db: &Db, price: &Self::Args) -> Self::Out {
            EVALS.fetch_add(1, Ordering::AcqRel);
            db.query::<BurritoPrice>(price) + 2
        }
    }

    struct BurritoCount;
    impl Input for BurritoCount {
        type Value = usize;
    }

    struct TotalPrice;
    impl Sig for TotalPrice {
        type Args = (InputId<BurritoPrice>, InputId<BurritoCount>);
        type Out = usize;
    }
    impl Query for TotalPrice {
        fn eval(db: &Db, args: &Self::Args) -> Self::Out {
            EVALS.fetch_add(1, Ordering::AcqRel);
            let (price, count) = args;
            db.query::<BurritoPriceWithShipping>(price) * db.query::<BurritoCount>(count)
        }
    }

    struct SalsaPerBurrito;
    impl Input for SalsaPerBurrito {
        type Value = usize;
    }

    struct SalsaInOrder;
    impl Sig for SalsaInOrder {
        type Args = (InputId<SalsaPerBurrito>, InputId<BurritoCount>);
        type Out = usize;
    }
    impl Query for SalsaInOrder {
        fn eval(db: &Db, args: &Self::Args) -> Self::Out {
            EVALS.fetch_add(1, Ordering::AcqRel);
            let (salsa_per, count) = args;
            db.query::<BurritoCount>(count) * db.query::<SalsaPerBurrito>(salsa_per)
        }
    }

    let mut db = Db::default();
    assert_eq!(EVALS.load(Ordering::Acquire), 0);
    let price = db.new_input::<BurritoPrice>(8);
    let ship_price = db.query::<BurritoPriceWithShipping>(&price);
    assert_eq!((EVALS.load(Ordering::Acquire), ship_price), (1, 10));
    let count = db.new_input::<BurritoCount>(3);
    let total = db.query::<TotalPrice>(&(price, count));
    assert_eq!((EVALS.load(Ordering::Acquire), total), (3, 30));
    let salsa_per = db.new_input::<SalsaPerBurrito>(40);
    let salsa_order = db.query::<SalsaInOrder>(&(salsa_per, count));
    assert_eq!((EVALS.load(Ordering::Acquire), salsa_order), (4, 120));

    let discount_price = db.new_input::<BurritoPrice>(4);
    let discount_ship_price = db.query::<BurritoPriceWithShipping>(&discount_price);
    assert_eq!((EVALS.load(Ordering::Acquire), discount_ship_price), (5, 6));
    let discount_total = db.query::<TotalPrice>(&(discount_price, count));
    assert_eq!((EVALS.load(Ordering::Acquire), discount_total), (7, 18));
    let salsa_in_order = db.query::<SalsaInOrder>(&(salsa_per, count));
    assert_eq!((EVALS.load(Ordering::Acquire), salsa_in_order), (8, 120));

    db.set_input(count, 5);
    let discount_ship_price = db.query::<BurritoPriceWithShipping>(&discount_price);
    assert_eq!((EVALS.load(Ordering::Acquire), discount_ship_price), (9, 6));
    let discount_total = db.query::<TotalPrice>(&(discount_price, count));
    assert_eq!((EVALS.load(Ordering::Acquire), discount_total), (11, 30));
    let salsa_in_order = db.query::<SalsaInOrder>(&(salsa_per, count));
    assert_eq!((EVALS.load(Ordering::Acquire), salsa_in_order), (12, 200));
}
