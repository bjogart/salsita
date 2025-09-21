use crate::Db;
use crate::Query;
use crate::Sig;

#[test]
#[should_panic]
fn cycles_panic() {
    struct Cycle;
    impl Sig for Cycle {
        type Args = ();
        type Output = ();
    }
    impl Query for Cycle {
        fn eval(db: &Db, (): &Self::Args) -> Self::Output {
            db.query::<Self>(&())
        }
    }

    Db::default().query::<Cycle>(&());
}
