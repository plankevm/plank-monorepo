use plank_core::CheckedConvertTo;

use crate::{
    op_graph::{OpGraph, OpNodeId, OpSet, ValueNodeId},
    stack::{ScheduleConfig, StackOps, TrackedStack},
    state::is_last_use,
};

fn dedup_unsorted<T: PartialEq>(values: &mut Vec<T>) {
    let mut i = 0;
    while i < values.len() {
        let mut j = i + 1;
        while j < values.len() {
            if values[i] == values[j] {
                values.swap_remove(j);
            } else {
                j += 1;
            }
        }
        i += 1;
    }
}

pub(crate) fn greedy_schedule_op<Sink: FnMut(StackOps)>(
    config: ScheduleConfig,
    stack: &mut TrackedStack<'_, Sink>,
    graph: &OpGraph,
    op_id: OpNodeId,
    values_buf: &mut Vec<ValueNodeId>,
    complete: OpSet<'_>,
) {
    let op = graph.get_op(op_id);

    let unique_last_uses = {
        values_buf.clear();
        for &value in op.inputs_fifo {
            if is_last_use(graph, complete, value) && !values_buf.contains(&value) {
                values_buf.push(value);
            }
        }
        values_buf.len().convert::<u16>()
    };

    let target_depth = stack.len() + op.inputs_fifo.len().convert::<u16>() - unique_last_uses;

    stack.op(graph, op_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_dedup_equals<T: PartialEq + std::fmt::Debug, const N: usize>(
        mut start: Vec<T>,
        expected: [T; N],
    ) {
        for i in 0..N {
            for j in i + 1..N {
                assert_ne!(expected[i], expected[j], "expected contains duplicates");
            }
        }
        dedup_unsorted(&mut start);
        assert_eq!(&start, expected.as_slice(), "deduped != expected");
    }

    #[test]
    fn test_dedup_unsorted() {
        assert_dedup_equals::<u32, _>(vec![], []);
        assert_dedup_equals(vec![1, 3, 2], [1, 3, 2]);
        assert_dedup_equals(vec![1, 3, 2, 3], [1, 3, 2]);
        assert_dedup_equals(vec![3, 1, 3, 3, 2], [3, 1, 2]);
        assert_dedup_equals(vec![1, 1, 1, 1], [1]);
    }
}
