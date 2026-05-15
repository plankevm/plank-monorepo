//! ## Code Layout
//!
//! A given contract object's code will be laid out as follows:
//!
//! ```txt
//! @initcode_start [implicit]
//!     (initcode)
//! @init_only_data_start
//!     (data)*
//! @runcode_start \
//!     (runcode)   |
//! @data_start     | Runtime
//!     (data)*     |
//! @initcode_end  /
//! ```

use plank_core::{IncIterable, Span};
use sir_assembler::MarkId;
use sir_data::{EthIRProgram, Idx};

#[derive(Debug)]
pub(crate) struct MarkMap {
    pub next_mark_id: MarkId,

    pub init_only_data_start: MarkId,
    pub runcode_start: MarkId,
    pub data_start: MarkId,
    pub initcode_end: MarkId,

    pub data_marks: Span<MarkId>,
}

impl MarkMap {
    pub fn new(ir: &EthIRProgram) -> Self {
        let mut next_mark_id = MarkId::ZERO;

        let init_only_data_start = next_mark_id.get_and_inc();
        let runcode_start = next_mark_id.get_and_inc();
        let data_start = next_mark_id.get_and_inc();
        let initcode_end = next_mark_id.get_and_inc();
        let data_marks = Self::alloc_id_span(&mut next_mark_id, ir.data_segments.len());

        Self {
            next_mark_id,

            init_only_data_start,
            runcode_start,
            data_start,
            initcode_end,

            data_marks,
        }
    }

    pub fn alloc_id_span(next_mark_id: &mut MarkId, size: usize) -> Span<MarkId> {
        let start = *next_mark_id;
        let end = start + u32::try_from(size).expect("mark span size overflow");
        *next_mark_id = end;
        Span::new(start, end)
    }

    pub fn runcode(&self) -> Span<MarkId> {
        Span::new(self.runcode_start, self.initcode_end)
    }
}
