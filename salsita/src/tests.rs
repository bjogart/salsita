use crate::Db;
use crate::Query;

#[test]
#[should_panic]
fn cycles_panic() {
    struct Cycle;
    impl Query for Cycle {
        type Args = ();
        type Out = ();
        fn eval(db: &Db, (): &Self::Args) -> Self::Out {
            db.query::<Self>(&())
        }
    }

    Db::default().query::<Cycle>(&());
}
