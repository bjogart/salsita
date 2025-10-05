use crate::Db;
use crate::Query;
use crate::metrics::Metrics;

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
