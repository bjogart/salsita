use crate::bench;
use core::hint::black_box;
use salsita::Db;
use salsita::Query;
use salsita::metrics::Metrics;

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

#[test]
fn star10() {
    let _ = black_box(bench::star10::<1>());
}

#[test]
fn star30() {
    let _ = black_box(bench::star30::<1>());
}

#[test]
fn star100() {
    let _ = black_box(bench::star100::<1>());
}

#[test]
fn chain5() {
    let _ = black_box(bench::chain5::<1>());
}

#[test]
fn chain25() {
    let _ = black_box(bench::chain25::<1>());
}

#[test]
fn chain100() {
    let _ = black_box(bench::chain100::<1>());
}

#[test]
fn tree_k3d2() {
    let _ = black_box(bench::tree_k3d2::<1>());
}

#[test]
fn tree_k3d3() {
    let _ = black_box(bench::tree_k3d3::<1>());
}

#[test]
fn tree_k3d4() {
    let _ = black_box(bench::tree_k3d4::<1>());
}

#[test]
fn hourglass3() {
    let _ = black_box(bench::hourglass3::<1>());
}

#[test]
fn hourglass6() {
    let _ = black_box(bench::hourglass6::<1>());
}

#[test]
fn hourglass9() {
    let _ = black_box(bench::hourglass9::<1>());
}
