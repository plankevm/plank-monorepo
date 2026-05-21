//! ## Code Layout
//!
//! A given contract object's code will be laid out as follows:
//!
//! ```txt
//! @initcode_start [implicit]
//!     (initcode)
//! @init_only_data_start [implicit]
//!     (data)*
//! @runcode_start         \
//!     (runcode)           |
//! @data_start [implicit]  | Runtime
//!     (data)*             |
//! @initcode_end          /
//! ```

use std::marker::PhantomData;

use plank_core::{Idx, IncIterable, Span};
use sir_assembler::MarkId;
use sir_data::{DataId, EthIRProgram};

#[derive(Debug, Clone, Copy)]
pub(crate) struct IndexableMarkSpan<I: Idx> {
    span: Span<MarkId>,
    _key: PhantomData<I>,
}

impl<I: Idx> IndexableMarkSpan<I> {
    pub fn get(self, index: I) -> MarkId {
        let mark = self.span.start + index.get();
        assert!(mark < self.span.end, "unexpected ID");
        mark
    }
}

#[derive(Debug)]
pub(crate) struct MarkMap {
    pub next_mark_id: MarkId,

    pub runcode_start: MarkId,
    pub initcode_end: MarkId,

    pub datas: IndexableMarkSpan<DataId>,
}

impl MarkMap {
    pub fn new(ir: &EthIRProgram) -> Self {
        let mut next_mark_id = MarkId::ZERO;

        let runcode_start = next_mark_id.get_and_inc();
        let initcode_end = next_mark_id.get_and_inc();
        let datas = Self::alloc_map(&mut next_mark_id, ir.data_segments.len());

        Self { next_mark_id, runcode_start, initcode_end, datas }
    }

    pub fn alloc_map<I: Idx>(next_mark_id: &mut MarkId, size: usize) -> IndexableMarkSpan<I> {
        let start = *next_mark_id;
        let end = start + u32::try_from(size).expect("mark span size overflow");
        *next_mark_id = end;
        IndexableMarkSpan { span: Span::new(start, end), _key: PhantomData }
    }

    pub fn runcode(&self) -> Span<MarkId> {
        Span::new(self.runcode_start, self.initcode_end)
    }
}
