use crate::macros::Add3;
use crate::macros::Add6;
use crate::macros::Add9;
use crate::macros::Add10;
use crate::macros::Add27;
use crate::macros::Add30;
use crate::macros::Add100;
use crate::macros::Dep1;
use crate::macros::Dep3;
use crate::macros::Dep6;
use crate::macros::Dep9;
use crate::macros::Dep10;
use crate::macros::Dep27;
use crate::macros::Dep30;
use crate::macros::Dep100;
use crate::macros::Inc1;
use crate::macros::Inc1V2;
use crate::macros::Inc1V3;
use crate::macros::Inc1V4;
use crate::macros::Inc1V5;
use crate::macros::Inc1V6;
use crate::macros::Inc1V7;
use crate::macros::Inc1V8;
use crate::macros::Inc1V9;
use crate::macros::Inc1V10;
use crate::macros::Inc1V11;
use crate::macros::Inc1V12;
use crate::macros::Inc1V13;
use crate::macros::Inc1V14;
use crate::macros::Inc1V15;
use crate::macros::Inc1V16;
use crate::macros::Inc1V17;
use crate::macros::Inc1V18;
use crate::macros::Inc1V19;
use crate::macros::Inc1V20;
use crate::macros::Inc1V21;
use crate::macros::Inc1V22;
use crate::macros::Inc1V23;
use crate::macros::Inc1V24;
use crate::macros::Inc1V25;
use crate::macros::Inc1V26;
use crate::macros::Inc1V27;
use crate::macros::Inc1V28;
use crate::macros::Inc1V29;
use crate::macros::Inc1V30;
use crate::macros::Inc1V31;
use crate::macros::Inc1V32;
use crate::macros::Inc1V33;
use crate::macros::Inc1V34;
use crate::macros::Inc1V35;
use crate::macros::Inc1V36;
use crate::macros::Inc1V37;
use crate::macros::Inc1V38;
use crate::macros::Inc1V39;
use crate::macros::Inc1V40;
use crate::macros::Inc1V41;
use crate::macros::Inc1V42;
use crate::macros::Inc1V43;
use crate::macros::Inc1V44;
use crate::macros::Inc1V45;
use crate::macros::Inc1V46;
use crate::macros::Inc1V47;
use crate::macros::Inc1V48;
use crate::macros::Inc1V49;
use crate::macros::Inc1V50;
use crate::macros::Inc1V51;
use crate::macros::Inc1V52;
use crate::macros::Inc1V53;
use crate::macros::Inc1V54;
use crate::macros::Inc1V55;
use crate::macros::Inc1V56;
use crate::macros::Inc1V57;
use crate::macros::Inc1V58;
use crate::macros::Inc1V59;
use crate::macros::Inc1V60;
use crate::macros::Inc1V61;
use crate::macros::Inc1V62;
use crate::macros::Inc1V63;
use crate::macros::Inc1V64;
use crate::macros::Inc1V65;
use crate::macros::Inc1V66;
use crate::macros::Inc1V67;
use crate::macros::Inc1V68;
use crate::macros::Inc1V69;
use crate::macros::Inc1V70;
use crate::macros::Inc1V71;
use crate::macros::Inc1V72;
use crate::macros::Inc1V73;
use crate::macros::Inc1V74;
use crate::macros::Inc1V75;
use crate::macros::Inc1V76;
use crate::macros::Inc1V77;
use crate::macros::Inc1V78;
use crate::macros::Inc1V79;
use crate::macros::Inc1V80;
use crate::macros::Inc1V81;
use crate::macros::Inc1V82;
use crate::macros::Inc1V83;
use crate::macros::Inc1V84;
use crate::macros::Inc1V85;
use crate::macros::Inc1V86;
use crate::macros::Inc1V87;
use crate::macros::Inc1V88;
use crate::macros::Inc1V89;
use crate::macros::Inc1V90;
use crate::macros::Inc1V91;
use crate::macros::Inc1V92;
use crate::macros::Inc1V93;
use crate::macros::Inc1V94;
use crate::macros::Inc1V95;
use crate::macros::Inc1V96;
use crate::macros::Inc1V97;
use crate::macros::Inc1V98;
use crate::macros::Inc1V99;
use crate::macros::Inc1V100;
use crate::macros::Inp;
use crate::macros::Inp2;
use crate::macros::Inp3;
use crate::macros::Inp4;
use crate::macros::Inp5;
use crate::macros::Inp6;
use crate::macros::Inp7;
use crate::macros::Inp8;
use crate::macros::Inp9;
use crate::macros::Tuple3;
use crate::macros::Tuple6;
use crate::macros::Tuple9;
use crate::macros::Tuple10;
use crate::macros::Tuple27;
use crate::macros::Tuple30;
use crate::macros::Tuple100;
use core::fmt::Debug;
use core::iter;
use salsita::Db;
use salsita::metrics::PerfMetrics;
use salsita::query::Query;

const WARMUP_COUNT: usize = 3;

#[derive(serde::Serialize)]
pub(crate) struct Report(Vec<BenchReport>);

#[derive(serde::Serialize)]
pub(crate) struct BenchReport {
    name: String,
    metrics: GraphMetrics,
}

#[derive(serde::Serialize)]
pub(crate) struct GraphMetrics {
    /// Measurements for a full build scenario.
    ///
    /// Starting with an empty `Db`, call the sink query once, causing every
    /// reachable query to be evaluated and inserted into the memo table. This
    /// measures the cost of an initial full build (time to register memos &
    /// evaluate nodes).
    cold: ScenarioMetrics,
    /// Measurements for a no-op scenario.
    ///
    /// Starting with a fresh `Db` that is already populated/memoized for the
    /// graph, call the sink query. No inputs are changed as part of this
    /// scenario. This measures steady-state read/query cost when the memo table
    /// is warm and every `Db::query` call is a memo hit.
    memo: ScenarioMetrics,
    /// Measurements for an incremental scenario.
    ///
    /// Starting with a fresh `Db` populated with memos for the whole graph,
    /// change exactly one input, then call the sink query. This measures
    /// incremental recomputation cost for the changed inputs and its
    /// dependents. Everything else is memoized.
    update: ScenarioMetrics,
}

#[derive(serde::Serialize)]
struct ScenarioMetrics {
    counts: Counts,
    timings: Vec<Timings>,
}

/// Query/evaluation counts for a single run.
///
/// All counts are recorded by [`PerfMetrics`] instrumentation.
#[derive(serde::Serialize)]
struct Counts {
    /// *Every* `Db::query::<Q>` invocation that occurred during the run,
    /// including nested calls made from within `Query::eval`.
    query: usize,
    /// The number of times `Query::eval` executed (i.e., cache misses /
    /// re-evaluations). Does *not* include memo hits where `eval` did not run.
    eval: usize,
}

/// Timing measurements for a single run.
///
/// All times are in **nanoseconds**, recorded by [`PerfMetrics`] instrumentation.
#[derive(serde::Serialize)]
struct Timings {
    /// Wall-clock time measured for the top-level `Db::query` call that the
    /// scenario invoked. Includes memo lookups, bookkeeping, invalidation
    /// handling triggered by each query, and the `eval` work described below.
    query: u64,
    /// Sum of time spent executing `Query::eval` across the whole call tree.
    /// Because `Query::eval` implementations may recursively call
    /// `db.query::<...>` (and those queries may themselves call `eval`),
    /// `eval_time_ns` includes the nested `eval` time in the entire evaluation
    /// tree. In other words, `eval_time_ns` is the total CPU time spent
    /// *inside* `eval` implementations.
    eval: u64,
}

type Sink3<D1, D2, D3> = Dep3<Add3, D1, D2, D3>;

type Sink6<D1, D2, D3, D4, D5, D6> = Dep6<Add6, D1, D2, D3, D4, D5, D6>;

type Sink9<D1, D2, D3, D4, D5, D6, D7, D8, D9> = Dep9<Add9, D1, D2, D3, D4, D5, D6, D7, D8, D9>;

type Sink10<D1, D2, D3, D4, D5, D6, D7, D8, D9, D10> =
    Dep10<Add10, D1, D2, D3, D4, D5, D6, D7, D8, D9, D10>;

type Sink27<
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
> = Dep27<
    Add27,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
>;

type Sink30<
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
> = Dep30<
    Add30,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
>;

type Sink100<
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
    D31,
    D32,
    D33,
    D34,
    D35,
    D36,
    D37,
    D38,
    D39,
    D40,
    D41,
    D42,
    D43,
    D44,
    D45,
    D46,
    D47,
    D48,
    D49,
    D50,
    D51,
    D52,
    D53,
    D54,
    D55,
    D56,
    D57,
    D58,
    D59,
    D60,
    D61,
    D62,
    D63,
    D64,
    D65,
    D66,
    D67,
    D68,
    D69,
    D70,
    D71,
    D72,
    D73,
    D74,
    D75,
    D76,
    D77,
    D78,
    D79,
    D80,
    D81,
    D82,
    D83,
    D84,
    D85,
    D86,
    D87,
    D88,
    D89,
    D90,
    D91,
    D92,
    D93,
    D94,
    D95,
    D96,
    D97,
    D98,
    D99,
    D100,
> = Dep100<
    Add100,
    D1,
    D2,
    D3,
    D4,
    D5,
    D6,
    D7,
    D8,
    D9,
    D10,
    D11,
    D12,
    D13,
    D14,
    D15,
    D16,
    D17,
    D18,
    D19,
    D20,
    D21,
    D22,
    D23,
    D24,
    D25,
    D26,
    D27,
    D28,
    D29,
    D30,
    D31,
    D32,
    D33,
    D34,
    D35,
    D36,
    D37,
    D38,
    D39,
    D40,
    D41,
    D42,
    D43,
    D44,
    D45,
    D46,
    D47,
    D48,
    D49,
    D50,
    D51,
    D52,
    D53,
    D54,
    D55,
    D56,
    D57,
    D58,
    D59,
    D60,
    D61,
    D62,
    D63,
    D64,
    D65,
    D66,
    D67,
    D68,
    D69,
    D70,
    D71,
    D72,
    D73,
    D74,
    D75,
    D76,
    D77,
    D78,
    D79,
    D80,
    D81,
    D82,
    D83,
    D84,
    D85,
    D86,
    D87,
    D88,
    D89,
    D90,
    D91,
    D92,
    D93,
    D94,
    D95,
    D96,
    D97,
    D98,
    D99,
    D100,
>;
type Branch<O> = Dep1<O, Inp>;

/// Star-shaped graph variants (single input -> many dependents). `starN`
/// means one input node feeds N dependent nodes (fanout = N).
///
/// This is useful to stress fanout and reveal invalidation cost when a
/// single input change causes re-evaluation of many dependents.
type Star10 = Sink10<
    Branch<Inc1>,
    Branch<Inc1V2>,
    Branch<Inc1V3>,
    Branch<Inc1V4>,
    Branch<Inc1V5>,
    Branch<Inc1V6>,
    Branch<Inc1V7>,
    Branch<Inc1V8>,
    Branch<Inc1V9>,
    Branch<Inc1V10>,
>;

type Star30 = Sink30<
    Branch<Inc1>,
    Branch<Inc1V2>,
    Branch<Inc1V3>,
    Branch<Inc1V4>,
    Branch<Inc1V5>,
    Branch<Inc1V6>,
    Branch<Inc1V7>,
    Branch<Inc1V8>,
    Branch<Inc1V9>,
    Branch<Inc1V10>,
    Branch<Inc1V11>,
    Branch<Inc1V12>,
    Branch<Inc1V13>,
    Branch<Inc1V14>,
    Branch<Inc1V15>,
    Branch<Inc1V16>,
    Branch<Inc1V17>,
    Branch<Inc1V18>,
    Branch<Inc1V19>,
    Branch<Inc1V20>,
    Branch<Inc1V21>,
    Branch<Inc1V22>,
    Branch<Inc1V23>,
    Branch<Inc1V24>,
    Branch<Inc1V25>,
    Branch<Inc1V26>,
    Branch<Inc1V27>,
    Branch<Inc1V28>,
    Branch<Inc1V29>,
    Branch<Inc1V30>,
>;

type Star100 = Sink100<
    Branch<Inc1>,
    Branch<Inc1V2>,
    Branch<Inc1V3>,
    Branch<Inc1V4>,
    Branch<Inc1V5>,
    Branch<Inc1V6>,
    Branch<Inc1V7>,
    Branch<Inc1V8>,
    Branch<Inc1V9>,
    Branch<Inc1V10>,
    Branch<Inc1V11>,
    Branch<Inc1V12>,
    Branch<Inc1V13>,
    Branch<Inc1V14>,
    Branch<Inc1V15>,
    Branch<Inc1V16>,
    Branch<Inc1V17>,
    Branch<Inc1V18>,
    Branch<Inc1V19>,
    Branch<Inc1V20>,
    Branch<Inc1V21>,
    Branch<Inc1V22>,
    Branch<Inc1V23>,
    Branch<Inc1V24>,
    Branch<Inc1V25>,
    Branch<Inc1V26>,
    Branch<Inc1V27>,
    Branch<Inc1V28>,
    Branch<Inc1V29>,
    Branch<Inc1V30>,
    Branch<Inc1V31>,
    Branch<Inc1V32>,
    Branch<Inc1V33>,
    Branch<Inc1V34>,
    Branch<Inc1V35>,
    Branch<Inc1V36>,
    Branch<Inc1V37>,
    Branch<Inc1V38>,
    Branch<Inc1V39>,
    Branch<Inc1V40>,
    Branch<Inc1V41>,
    Branch<Inc1V42>,
    Branch<Inc1V43>,
    Branch<Inc1V44>,
    Branch<Inc1V45>,
    Branch<Inc1V46>,
    Branch<Inc1V47>,
    Branch<Inc1V48>,
    Branch<Inc1V49>,
    Branch<Inc1V50>,
    Branch<Inc1V51>,
    Branch<Inc1V52>,
    Branch<Inc1V53>,
    Branch<Inc1V54>,
    Branch<Inc1V55>,
    Branch<Inc1V56>,
    Branch<Inc1V57>,
    Branch<Inc1V58>,
    Branch<Inc1V59>,
    Branch<Inc1V60>,
    Branch<Inc1V61>,
    Branch<Inc1V62>,
    Branch<Inc1V63>,
    Branch<Inc1V64>,
    Branch<Inc1V65>,
    Branch<Inc1V66>,
    Branch<Inc1V67>,
    Branch<Inc1V68>,
    Branch<Inc1V69>,
    Branch<Inc1V70>,
    Branch<Inc1V71>,
    Branch<Inc1V72>,
    Branch<Inc1V73>,
    Branch<Inc1V74>,
    Branch<Inc1V75>,
    Branch<Inc1V76>,
    Branch<Inc1V77>,
    Branch<Inc1V78>,
    Branch<Inc1V79>,
    Branch<Inc1V80>,
    Branch<Inc1V81>,
    Branch<Inc1V82>,
    Branch<Inc1V83>,
    Branch<Inc1V84>,
    Branch<Inc1V85>,
    Branch<Inc1V86>,
    Branch<Inc1V87>,
    Branch<Inc1V88>,
    Branch<Inc1V89>,
    Branch<Inc1V90>,
    Branch<Inc1V91>,
    Branch<Inc1V92>,
    Branch<Inc1V93>,
    Branch<Inc1V94>,
    Branch<Inc1V95>,
    Branch<Inc1V96>,
    Branch<Inc1V97>,
    Branch<Inc1V98>,
    Branch<Inc1V99>,
    Branch<Inc1V100>,
>;

type Link<D> = Dep1<Inc1, D>;

type Link5<D> = Link<Link<Link<Link<Link<D>>>>>;

type Link25<D> = Link5<Link5<Link5<Link5<Link5<D>>>>>;

/// Chain-shaped graph variants `(A -> B -> C -> ...)`. `chainN` denotes a
/// linear chain of N distinct queries where each node depends on the
/// previous node.
///
/// This exercises propagation through long narrow dependency paths and
/// exposes per-level recursion/overhead.
type Chain5 = Link5<Inp>;

type Chain25 = Link25<Inp>;

type Chain100 = Link25<Link25<Link25<Link25<Inp>>>>;

type Root = Inp;

type Tree<O, P> = Dep1<O, P>;

type TreeD1I1 = Tree<Inc1, Root>;

type TreeD1I2 = Tree<Inc1V2, Root>;

type TreeD1I3 = Tree<Inc1V3, Root>;

type TreeD2I1 = Tree<Inc1V4, TreeD1I1>;

type TreeD2I2 = Tree<Inc1V5, TreeD1I1>;

type TreeD2I3 = Tree<Inc1V6, TreeD1I1>;

type TreeD2I4 = Tree<Inc1V7, TreeD1I2>;

type TreeD2I5 = Tree<Inc1V8, TreeD1I2>;

type TreeD2I6 = Tree<Inc1V9, TreeD1I2>;

type TreeD2I7 = Tree<Inc1V10, TreeD1I3>;

type TreeD2I8 = Tree<Inc1V11, TreeD1I3>;

type TreeD2I9 = Tree<Inc1V12, TreeD1I3>;

type TreeD3I1 = Tree<Inc1V13, TreeD2I1>;

type TreeD3I2 = Tree<Inc1V14, TreeD2I1>;

type TreeD3I3 = Tree<Inc1V15, TreeD2I1>;

type TreeD3I4 = Tree<Inc1V16, TreeD2I2>;

type TreeD3I5 = Tree<Inc1V17, TreeD2I2>;

type TreeD3I6 = Tree<Inc1V18, TreeD2I2>;

type TreeD3I7 = Tree<Inc1V19, TreeD2I3>;

type TreeD3I8 = Tree<Inc1V20, TreeD2I3>;

type TreeD3I9 = Tree<Inc1V21, TreeD2I3>;

type TreeD3I10 = Tree<Inc1V22, TreeD2I4>;

type TreeD3I11 = Tree<Inc1V23, TreeD2I4>;

type TreeD3I12 = Tree<Inc1V24, TreeD2I4>;

type TreeD3I13 = Tree<Inc1V25, TreeD2I5>;

type TreeD3I14 = Tree<Inc1V26, TreeD2I5>;

type TreeD3I15 = Tree<Inc1V27, TreeD2I5>;

type TreeD3I16 = Tree<Inc1V28, TreeD2I6>;

type TreeD3I17 = Tree<Inc1V29, TreeD2I6>;

type TreeD3I18 = Tree<Inc1V30, TreeD2I6>;

type TreeD3I19 = Tree<Inc1V31, TreeD2I7>;

type TreeD3I20 = Tree<Inc1V32, TreeD2I7>;

type TreeD3I21 = Tree<Inc1V33, TreeD2I7>;

type TreeD3I22 = Tree<Inc1V34, TreeD2I8>;

type TreeD3I23 = Tree<Inc1V35, TreeD2I8>;

type TreeD3I24 = Tree<Inc1V36, TreeD2I8>;

type TreeD3I25 = Tree<Inc1V37, TreeD2I9>;

type TreeD3I26 = Tree<Inc1V38, TreeD2I9>;

type TreeD3I27 = Tree<Inc1V39, TreeD2I9>;

/// Balanced k-ary trees of small depth. `tree_kKdD` is a K-ary tree of
/// depth D. These combine branching and depth in a controlled manner.
///
/// These represents hierarchical graphs (such as AST / module dependency
/// shapes). Useful to measure bulk recomputation costs when internal nodes
/// invalidate whole subtrees.
type TreeK3D2 = Sink3<TreeD1I1, TreeD1I2, TreeD1I3>;

type TreeK3D3 =
    Sink9<TreeD2I1, TreeD2I2, TreeD2I3, TreeD2I4, TreeD2I5, TreeD2I6, TreeD2I7, TreeD2I8, TreeD2I9>;

type TreeK3D4 = Sink27<
    TreeD3I1,
    TreeD3I2,
    TreeD3I3,
    TreeD3I4,
    TreeD3I5,
    TreeD3I6,
    TreeD3I7,
    TreeD3I8,
    TreeD3I9,
    TreeD3I10,
    TreeD3I11,
    TreeD3I12,
    TreeD3I13,
    TreeD3I14,
    TreeD3I15,
    TreeD3I16,
    TreeD3I17,
    TreeD3I18,
    TreeD3I19,
    TreeD3I20,
    TreeD3I21,
    TreeD3I22,
    TreeD3I23,
    TreeD3I24,
    TreeD3I25,
    TreeD3I26,
    TreeD3I27,
>;

type FanIn<I> = Dep1<Inc1, I>;

type FanOut<O, H> = Dep1<O, H>;

type Hub3 = Dep3<Add3, FanIn<Inp>, FanIn<Inp2>, FanIn<Inp3>>;

type Hub6 = Dep6<Add6, FanIn<Inp>, FanIn<Inp2>, FanIn<Inp3>, FanIn<Inp4>, FanIn<Inp5>, FanIn<Inp6>>;

type Hub9 = Dep9<
    Add9,
    FanIn<Inp>,
    FanIn<Inp2>,
    FanIn<Inp3>,
    FanIn<Inp4>,
    FanIn<Inp5>,
    FanIn<Inp6>,
    FanIn<Inp7>,
    FanIn<Inp8>,
    FanIn<Inp9>,
>;

/// Hourglass-shaped graph. `hourglassN` means N inputs converge into a
/// single intermediate region and then diverge again into N outputs.
///
/// This kind of graph stresses shared subexpressions and reuse. A correct
/// incremental engine should compute the shared region once and reuse it.
type Hourglass3 = Sink3<FanOut<Inc1, Hub3>, FanOut<Inc1V2, Hub3>, FanOut<Inc1V3, Hub3>>;

type Hourglass6 = Sink6<
    FanOut<Inc1, Hub6>,
    FanOut<Inc1V2, Hub6>,
    FanOut<Inc1V3, Hub6>,
    FanOut<Inc1V4, Hub6>,
    FanOut<Inc1V5, Hub6>,
    FanOut<Inc1V6, Hub6>,
>;

type Hourglass9 = Sink9<
    FanOut<Inc1, Hub9>,
    FanOut<Inc1V2, Hub9>,
    FanOut<Inc1V3, Hub9>,
    FanOut<Inc1V4, Hub9>,
    FanOut<Inc1V5, Hub9>,
    FanOut<Inc1V6, Hub9>,
    FanOut<Inc1V7, Hub9>,
    FanOut<Inc1V8, Hub9>,
    FanOut<Inc1V9, Hub9>,
>;

impl Report {
    pub(crate) fn new<const N: usize>() -> Self {
        Self(vec![
            star10::<N>(),
            star30::<N>(),
            star100::<N>(),
            chain5::<N>(),
            chain25::<N>(),
            chain100::<N>(),
            tree_k3d2::<N>(),
            tree_k3d3::<N>(),
            tree_k3d4::<N>(),
            hourglass3::<N>(),
            hourglass6::<N>(),
            hourglass9::<N>(),
        ])
    }
}

pub(crate) fn star10<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Star10>(
        |db| {
            let inp = db.new_input::<Inp>(1);
            Tuple10(inp, inp, inp, inp, inp, inp, inp, inp, inp, inp)
        },
        |db, Tuple10(inp, _, _, _, _, _, _, _, _, _)| db.set_input(*inp, 2),
        20,
        30,
    );
    BenchReport::new("star10", metrics)
}

pub(crate) fn star30<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Star30>(
        |db| {
            let inp = db.new_input::<Inp>(1);
            Tuple30(
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
            )
        },
        |db,
         Tuple30(
            inp,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
        )| db.set_input(*inp, 2),
        60,
        90,
    );
    BenchReport::new("star30", metrics)
}

pub(crate) fn star100<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Star100>(
        |db| {
            let inp = db.new_input::<Inp>(1);
            Tuple100(
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp,
            )
        },
        |db,
         Tuple100(
            inp,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
        )| db.set_input(*inp, 2),
        200,
        300,
    );
    BenchReport::new("star100", metrics)
}

pub(crate) fn chain5<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Chain5>(
        |db| db.new_input::<Inp>(0),
        |db, inp| db.set_input(*inp, 1),
        5,
        6,
    );
    BenchReport::new("chain5", metrics)
}

pub(crate) fn chain25<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Chain25>(
        |db| db.new_input::<Inp>(0),
        |db, inp| db.set_input(*inp, 1),
        25,
        26,
    );
    BenchReport::new("chain25", metrics)
}

pub(crate) fn chain100<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Chain100>(
        |db| db.new_input::<Inp>(0),
        |db, inp| db.set_input(*inp, 1),
        100,
        101,
    );
    BenchReport::new("chain100", metrics)
}

pub(crate) fn tree_k3d2<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, TreeK3D2>(
        |db| {
            let inp = db.new_input::<Inp>(0);
            Tuple3(inp, inp, inp)
        },
        |db, Tuple3(inp, _, _)| db.set_input(*inp, 1),
        3,
        6,
    );
    BenchReport::new("tree_k3d2", metrics)
}

pub(crate) fn tree_k3d3<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, TreeK3D3>(
        |db| {
            let inp = db.new_input::<Inp>(0);
            Tuple9(inp, inp, inp, inp, inp, inp, inp, inp, inp)
        },
        |db, Tuple9(inp, _, _, _, _, _, _, _, _)| db.set_input(*inp, 1),
        18,
        27,
    );
    BenchReport::new("tree_k3d3", metrics)
}

pub(crate) fn tree_k3d4<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, TreeK3D4>(
        |db| {
            let inp = db.new_input::<Inp>(0);
            Tuple27(
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
                inp, inp, inp, inp, inp, inp, inp, inp, inp, inp, inp,
            )
        },
        |db,
         Tuple27(
            inp,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
            _,
        )| { db.set_input(*inp, 1) },
        81,
        108,
    );
    BenchReport::new("tree_k3d4", metrics)
}

pub(crate) fn hourglass3<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Hourglass3>(
        |db| {
            let inp = Tuple3(
                db.new_input::<Inp>(2),
                db.new_input::<Inp2>(1),
                db.new_input::<Inp3>(1),
            );
            Tuple3(inp, inp, inp)
        },
        |db, Tuple3(Tuple3(inp1, inp2, _), _, _)| {
            db.set_input(*inp1, 1);
            db.set_input(*inp2, 2);
        },
        24,
        24,
    );
    BenchReport::new("hourglass3", metrics)
}

pub(crate) fn hourglass6<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Hourglass6>(
        |db| {
            let inp = Tuple6(
                db.new_input::<Inp>(2),
                db.new_input::<Inp2>(1),
                db.new_input::<Inp3>(1),
                db.new_input::<Inp4>(1),
                db.new_input::<Inp5>(1),
                db.new_input::<Inp6>(1),
            );
            Tuple6(inp, inp, inp, inp, inp, inp)
        },
        |db, Tuple6(Tuple6(inp1, inp2, _, _, _, _), _, _, _, _, _)| {
            db.set_input(*inp1, 1);
            db.set_input(*inp2, 2);
        },
        84,
        84,
    );
    BenchReport::new("hourglass6", metrics)
}

pub(crate) fn hourglass9<const N: usize>() -> BenchReport {
    let metrics = bench_graph::<N, Hourglass9>(
        |db| {
            let inp = Tuple9(
                db.new_input::<Inp>(2),
                db.new_input::<Inp2>(1),
                db.new_input::<Inp3>(1),
                db.new_input::<Inp4>(1),
                db.new_input::<Inp5>(1),
                db.new_input::<Inp6>(1),
                db.new_input::<Inp7>(1),
                db.new_input::<Inp8>(1),
                db.new_input::<Inp9>(1),
            );
            Tuple9(inp, inp, inp, inp, inp, inp, inp, inp, inp)
        },
        |db, Tuple9(Tuple9(inp1, inp2, _, _, _, _, _, _, _), _, _, _, _, _, _, _, _)| {
            db.set_input(*inp1, 1);
            db.set_input(*inp2, 2);
        },
        180,
        180,
    );
    BenchReport::new("hourglass9", metrics)
}

impl BenchReport {
    fn new(name: &'static str, metrics: GraphMetrics) -> Self {
        Self {
            name: name.to_owned(),
            metrics,
        }
    }
}

fn bench_graph<const N: usize, Sink>(
    alloc_inputs: impl Fn(&mut Db<PerfMetrics>) -> Sink::Args,
    update_inputs: impl Fn(&mut Db<PerfMetrics>, &Sink::Args),
    exp_cold: Sink::Out,
    exp_update: Sink::Out,
) -> GraphMetrics
where
    Sink: Query,
    Sink::Out: Copy + Eq + Debug,
{
    GraphMetrics {
        cold: bench_scenario::<N, _, _>(
            |_| {},
            |db, ()| {
                let inputs = alloc_inputs(db);
                db.query::<Sink>(&inputs)
            },
            exp_cold,
        ),
        memo: bench_scenario::<N, _, _>(
            |db| {
                let inputs = alloc_inputs(db);
                db.query::<Sink>(&inputs);
                inputs
            },
            |db, inputs| db.query::<Sink>(&inputs),
            exp_cold,
        ),
        update: bench_scenario::<N, _, _>(
            |db| {
                let inputs = alloc_inputs(db);
                db.query::<Sink>(&inputs);
                inputs
            },
            |db, inputs| {
                update_inputs(db, &inputs);
                db.query::<Sink>(&inputs)
            },
            exp_update,
        ),
    }
}

fn bench_scenario<const N: usize, Inp, Out>(
    init: impl Copy + Fn(&mut Db<PerfMetrics>) -> Inp,
    bench: impl Copy + Fn(&mut Db<PerfMetrics>, Inp) -> Out,
    exp: Out,
) -> ScenarioMetrics
where
    Out: Copy + Eq + Debug,
{
    let (_, counts, out) = bench_iter(init, bench);
    assert_eq!(out, exp);
    for _ in 0..WARMUP_COUNT {
        let (_, _, _) = bench_iter(init, bench);
    }
    let timings = iter::repeat_with(|| {
        let (timings, _, _) = bench_iter(init, bench);
        timings
    })
    .take(N)
    .collect();
    return ScenarioMetrics { counts, timings };

    fn bench_iter<T, Out>(
        init: impl Fn(&mut Db<PerfMetrics>) -> T,
        bench: impl Fn(&mut Db<PerfMetrics>, T) -> Out,
    ) -> (Timings, Counts, Out)
    where
        Out: Eq + Debug,
    {
        let mut db = Db::default();
        let v = init(&mut db);
        db.metrics().reset();
        let out = bench(&mut db, v);
        let m = db.metrics();
        (Timings::new(m), Counts::new(m), out)
    }
}

impl Counts {
    fn new(m: &PerfMetrics) -> Self {
        Self {
            query: m.query_count(),
            eval: m.eval_count(),
        }
    }
}

impl Timings {
    fn new(m: &PerfMetrics) -> Self {
        Self {
            query: m.query_time().as_nanos().try_into().unwrap_or(u64::MAX),
            eval: m.eval_time().as_nanos().try_into().unwrap_or(u64::MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::bench;
    use core::hint::black_box;

    #[test]
    fn star10() {
        black_box(bench::star10::<1>());
    }

    #[test]
    fn star30() {
        black_box(bench::star30::<1>());
    }

    #[test]
    fn star100() {
        black_box(bench::star100::<1>());
    }

    #[test]
    fn chain5() {
        black_box(bench::chain5::<1>());
    }

    #[test]
    fn chain25() {
        black_box(bench::chain25::<1>());
    }

    #[test]
    fn chain100() {
        black_box(bench::chain100::<1>());
    }

    #[test]
    fn tree_k3d2() {
        black_box(bench::tree_k3d2::<1>());
    }

    #[test]
    fn tree_k3d3() {
        black_box(bench::tree_k3d3::<1>());
    }

    #[test]
    fn tree_k3d4() {
        black_box(bench::tree_k3d4::<1>());
    }

    #[test]
    fn hourglass3() {
        black_box(bench::hourglass3::<1>());
    }

    #[test]
    fn hourglass6() {
        black_box(bench::hourglass6::<1>());
    }

    #[test]
    fn hourglass9() {
        black_box(bench::hourglass9::<1>());
    }
}
