use crate::Db;
use crate::Query;
use crate::Sig;

#[test]
// TODO replace `should_panic` with asserts and `catch_unwind` to ensure the same behavior for newly registered queries and queries previously registered.
#[should_panic]
fn cycles_panic() {
    struct Cycle;
    impl Sig for Cycle {
        type Args = ();
        type Out = ();
    }
    impl Query for Cycle {
        fn eval(db: &Db, (): &Self::Args) -> Self::Out {
            db.query::<Self>(&())
        }
    }

    Db::default().query::<Cycle>(&());
}
