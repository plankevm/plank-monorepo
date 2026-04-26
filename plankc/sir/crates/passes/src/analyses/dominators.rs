use hashbrown::HashSet;
use plank_core::{DenseIndexMap, LoopLimit};
use sir_data::{BasicBlockId, EthIRProgram, FunctionView, IndexVec, index_vec};

use crate::analyses::{AnalysesStore, Predecessors, cache::Analysis};

#[derive(Default)]
pub struct Dominators {
    inner: DenseIndexMap<BasicBlockId, BasicBlockId>,
}

impl Analysis for Dominators {
    // iterative dominator algorithm using RPO
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        let predecessors = store.predecessors(program);
        let rpo = store.reverse_post_order(program);
        self.inner.clear();
        for func in program.functions_iter() {
            compute_function_dominators(
                program,
                func,
                &predecessors,
                rpo.function_rpo(func.id()),
                &mut self.inner,
            );
        }
    }
}

impl Dominators {
    pub fn of(&self, bb: BasicBlockId) -> Option<BasicBlockId> {
        self.inner.get(bb).copied()
    }
}

#[derive(Default)]
pub struct DominanceFrontiers {
    inner: IndexVec<BasicBlockId, HashSet<BasicBlockId>>,
}

impl Analysis for DominanceFrontiers {
    fn compute(&mut self, program: &EthIRProgram, store: &AnalysesStore) {
        let dominators = store.dominators(program);
        let predecessors = store.predecessors(program);
        for set in self.inner.iter_mut() {
            set.clear();
        }
        self.inner.resize_with(program.basic_blocks.len(), HashSet::new);

        for (b, preds) in predecessors.enumerate() {
            if preds.len() < 2 {
                continue;
            }
            let Some(idom) = dominators.of(b) else {
                continue;
            };
            for &p in preds {
                if dominators.of(p).is_none() {
                    continue;
                }
                let mut runner = p;
                let mut limit = LoopLimit::new();
                while runner != idom {
                    limit.tick();
                    self.inner[runner].insert(b);
                    runner = dominators.of(runner).expect("reachable path");
                }
            }
        }
    }
}

impl DominanceFrontiers {
    pub fn of(&self, bb: BasicBlockId) -> &HashSet<BasicBlockId> {
        &self.inner[bb]
    }
}

fn compute_function_dominators(
    program: &EthIRProgram,
    function: FunctionView<'_>,
    predecessors: &Predecessors,
    rpo: &[BasicBlockId],
    dominators: &mut DenseIndexMap<BasicBlockId, BasicBlockId>,
) {
    let entry = function.entry().id();
    assert!(dominators.insert(entry, entry).is_none());

    let mut bb_to_rpo_pos = index_vec![0; program.basic_blocks.len()];
    for (pos, &basic_block) in rpo.iter().enumerate() {
        bb_to_rpo_pos[basic_block] = pos as u32;
    }

    let mut changed = true;
    let mut limit = LoopLimit::new();
    while changed {
        limit.tick();
        changed = false;
        for &bb in rpo[1..].iter() {
            let mut preds =
                predecessors.of(bb).iter().copied().filter(|&pred| dominators.contains(pred));
            let mut new_idom = preds.next().expect("non-entry block in RPO has no predecessors");
            for pred in preds {
                if dominators.contains(pred) {
                    new_idom = intersect(pred, new_idom, dominators, &bb_to_rpo_pos);
                }
            }
            changed |=
                dominators.insert(bb, new_idom).is_none_or(|prev_idom| prev_idom != new_idom);
        }
    }
}

fn intersect(
    bb1: BasicBlockId,
    bb2: BasicBlockId,
    dominators: &DenseIndexMap<BasicBlockId, BasicBlockId>,
    bb_to_rpo_pos: &IndexVec<BasicBlockId, u32>,
) -> BasicBlockId {
    let mut finger1 = bb1;
    let mut finger2 = bb2;
    let mut limit = LoopLimit::new();
    while finger1 != finger2 {
        limit.tick();
        while bb_to_rpo_pos[finger1] > bb_to_rpo_pos[finger2] {
            limit.tick();
            finger1 = dominators[finger1];
        }
        while bb_to_rpo_pos[finger2] > bb_to_rpo_pos[finger1] {
            limit.tick();
            finger2 = dominators[finger2];
        }
    }

    finger1
}

#[cfg(test)]
mod tests {
    use super::*;
    use sir_parser::{EmitConfig, parse_or_panic};

    fn bb(n: u32) -> BasicBlockId {
        BasicBlockId::new(n)
    }

    fn frontier_to_vec(df: &HashSet<BasicBlockId>) -> Vec<BasicBlockId> {
        let mut v: Vec<_> = df.iter().copied().collect();
        v.sort();
        v
    }

    fn make_store(program: &EthIRProgram) -> crate::AnalysesStore {
        let store = crate::AnalysesStore::default();
        store.dominance_frontiers(program);
        store
    }

    #[test]
    fn test_loop_back_edge() {
        // A → B → C → B (back-edge)
        //     |
        //     D
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    => @b
                }
                b {
                    x = const 1
                    => x ? @c : @d
                }
                c {
                    => @b
                }
                d {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), Some(bb(1))); // idom(C) = B
        assert_eq!(dominators.of(bb(3)), Some(bb(1))); // idom(D) = B
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![bb(1)]); // DF(B) = {B}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![bb(1)]); // DF(C) = {B}
        assert_eq!(frontier_to_vec(df.of(bb(3))), vec![]); // DF(D) = {}
    }

    #[test]
    fn test_linear_chain() {
        // A → B → C
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    => @b
                }
                b {
                    => @c
                }
                c {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), Some(bb(1))); // idom(C) = B
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![]); // DF(B) = {}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![]); // DF(C) = {}
    }

    #[test]
    fn test_diamond() {
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    x = const 1
                    => x ? @b : @c
                }
                b {
                    => @d
                }
                c {
                    => @d
                }
                d {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), Some(bb(0))); // idom(C) = A
        assert_eq!(dominators.of(bb(3)), Some(bb(0))); // idom(D) = A (not B or C)
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![bb(3)]); // DF(B) = {D}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![bb(3)]); // DF(C) = {D}
        assert_eq!(frontier_to_vec(df.of(bb(3))), vec![]); // DF(D) = {}
    }

    #[test]
    fn test_cross_edges() {
        //     A
        //    / \
        //   B   C
        //   |   |
        //   D → E (cross edge from D to E)
        //       |
        //       F
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    x = const 1
                    => x ? @b : @c
                }
                b {
                    => @d
                }
                c {
                    => @e
                }
                d {
                    => @e
                }
                e {
                    => @f
                }
                f {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), Some(bb(0))); // idom(C) = A
        assert_eq!(dominators.of(bb(3)), Some(bb(1))); // idom(D) = B
        assert_eq!(dominators.of(bb(4)), Some(bb(0))); // idom(E) = A (common dominator of C and D)
        assert_eq!(dominators.of(bb(5)), Some(bb(4))); // idom(F) = E
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![bb(4)]); // DF(B) = {E}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![bb(4)]); // DF(C) = {E}
        assert_eq!(frontier_to_vec(df.of(bb(3))), vec![bb(4)]); // DF(D) = {E}
        assert_eq!(frontier_to_vec(df.of(bb(4))), vec![]); // DF(E) = {}
        assert_eq!(frontier_to_vec(df.of(bb(5))), vec![]); // DF(F) = {}
    }

    #[test]
    fn test_nested_loops() {
        // A → B → C ⟷ D (inner loop C-D)
        //     ↑       ↓
        //     +───────E → F (exit)
        //     (D also → B via E, outer backedge)
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    => @b
                }
                b {
                    => @c
                }
                c {
                    => @d
                }
                d {
                    x = const 1
                    => x ? @c : @e
                }
                e {
                    y = const 1
                    => y ? @b : @f
                }
                f {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), Some(bb(1))); // idom(C) = B
        assert_eq!(dominators.of(bb(3)), Some(bb(2))); // idom(D) = C
        assert_eq!(dominators.of(bb(4)), Some(bb(3))); // idom(E) = D
        assert_eq!(dominators.of(bb(5)), Some(bb(4))); // idom(F) = E
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![bb(1)]); // DF(B) = {B}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![bb(1), bb(2)]); // DF(C) = {B, C}
        assert_eq!(frontier_to_vec(df.of(bb(3))), vec![bb(1), bb(2)]); // DF(D) = {B, C}
        assert_eq!(frontier_to_vec(df.of(bb(4))), vec![bb(1)]); // DF(E) = {B}
        assert_eq!(frontier_to_vec(df.of(bb(5))), vec![]); // DF(F) = {}
    }

    #[test]
    fn test_unreachable_block() {
        // A → B, C is in same function but unreachable
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    => @b
                }
                b {
                    stop
                }
                c {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), None); // C is unreachable
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![]); // DF(B) = {}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![]); // DF(C) = {}
    }

    #[test]
    fn test_multiple_entry_points() {
        // Two disconnected components: A → B, C → D
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    => @b
                }
                b {
                    stop
                }
            fn other:
                c {
                    => @d
                }
                d {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), Some(bb(2))); // idom(C) = C (entry of other)
        assert_eq!(dominators.of(bb(3)), Some(bb(2))); // idom(D) = C
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![]); // DF(B) = {}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![]); // DF(C) = {}
        assert_eq!(frontier_to_vec(df.of(bb(3))), vec![]); // DF(D) = {}
    }

    #[test]
    fn test_stacked_diamonds() {
        //     A
        //    / \
        //   B   C
        //    \ /
        //     D
        //    / \
        //   E   F
        //    \ /
        //     G
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    x = const 1
                    => x ? @b : @c
                }
                b {
                    => @d
                }
                c {
                    => @d
                }
                d {
                    y = const 1
                    => y ? @e : @f
                }
                e {
                    => @g
                }
                f {
                    => @g
                }
                g {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), Some(bb(0))); // idom(C) = A
        assert_eq!(dominators.of(bb(3)), Some(bb(0))); // idom(D) = A
        assert_eq!(dominators.of(bb(4)), Some(bb(3))); // idom(E) = D
        assert_eq!(dominators.of(bb(5)), Some(bb(3))); // idom(F) = D
        assert_eq!(dominators.of(bb(6)), Some(bb(3))); // idom(G) = D
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![bb(3)]); // DF(B) = {D}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![bb(3)]); // DF(C) = {D}
        assert_eq!(frontier_to_vec(df.of(bb(3))), vec![]); // DF(D) = {}
        assert_eq!(frontier_to_vec(df.of(bb(4))), vec![bb(6)]); // DF(E) = {G}
        assert_eq!(frontier_to_vec(df.of(bb(5))), vec![bb(6)]); // DF(F) = {G}
        assert_eq!(frontier_to_vec(df.of(bb(6))), vec![]); // DF(G) = {}
    }

    #[test]
    fn test_irreducible_cfg() {
        // Irreducible CFG - loop with multiple entries
        //     A
        //    / \
        //   B ↔ C
        //    \ /
        //     D
        let program = parse_or_panic(
            r#"
            fn init:
                a {
                    x = const 1
                    => x ? @b : @c
                }
                b {
                    y = const 1
                    => y ? @c : @d
                }
                c {
                    z = const 1
                    => z ? @b : @d
                }
                d {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = make_store(&program);
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(A) = A
        assert_eq!(dominators.of(bb(1)), Some(bb(0))); // idom(B) = A
        assert_eq!(dominators.of(bb(2)), Some(bb(0))); // idom(C) = A
        assert_eq!(dominators.of(bb(3)), Some(bb(0))); // idom(D) = A (common dominator of B and C paths)
        let df = store.dominance_frontiers(&program);
        assert_eq!(frontier_to_vec(df.of(bb(0))), vec![]); // DF(A) = {}
        assert_eq!(frontier_to_vec(df.of(bb(1))), vec![bb(2), bb(3)]); // DF(B) = {C, D}
        assert_eq!(frontier_to_vec(df.of(bb(2))), vec![bb(1), bb(3)]); // DF(C) = {B, D}
        assert_eq!(frontier_to_vec(df.of(bb(3))), vec![]); // DF(D) = {}
    }

    #[test]
    fn test_unreachable_predecessor() {
        //   entry → B → D (stop)
        //           |   ↑
        //           +→C-+
        //               ↑
        //   orphan ─────+ (unreachable)
        //
        // orphan is listed before B and C so it becomes preds[0] for D.
        // Regression test: the old algorithm unconditionally used preds[0] as
        // the initial idom candidate. When preds[0] was unreachable (no
        // computed dominator, RPO position 0), intersect would loop forever
        // comparing two blocks at the same RPO position.
        let program = sir_parser::parse_without_legalization(
            r#"
            fn init:
                entry {
                    => @b
                }
                orphan {
                    => @d
                }
                b {
                    x = const 1
                    => x ? @c : @d
                }
                c {
                    => @d
                }
                d {
                    stop
                }
            "#,
            EmitConfig::init_only(),
        );

        let store = crate::AnalysesStore::default();
        let dominators = store.dominators(&program);
        assert_eq!(dominators.of(bb(0)), Some(bb(0))); // idom(entry) = entry
        assert_eq!(dominators.of(bb(2)), Some(bb(0))); // idom(B) = entry
        assert_eq!(dominators.of(bb(3)), Some(bb(2))); // idom(C) = B
        assert_eq!(dominators.of(bb(4)), Some(bb(2))); // idom(D) = B
        assert_eq!(dominators.of(bb(1)), None); // orphan is unreachable
    }
}
