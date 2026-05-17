use alloy_primitives::U256;
use plank_core::{
    Idx, IndexVec, Span, const_print::const_assert_mem_size, index_vec, newtype_index,
};

mod display;
pub mod op;

const ASSUMED_MARK_COUNT_WITHOUT_HINT: usize = 128;
const MAX_ASSEMBLER_CONVERGENCE_ITERS: usize = 1024;

newtype_index! {
    pub struct MarkId;
    pub struct AsmBytesIdx;
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RefSize {
    S1 = 1,
    S2 = 2,
    S3 = 3,
    S4 = 4,
}

#[derive(Debug, Clone, Copy)]
struct DirectMarkRef {
    id: MarkId,
    set_size: Option<RefSize>,
    pushed: bool,
}

#[derive(Debug, Clone, Copy)]
enum StoredAsmSection {
    Mark(MarkId),
    Ops(Span<AsmBytesIdx>),
    Data(Span<AsmBytesIdx>),
    DirectMarkRef(DirectMarkRef),
    UnsizedPushedDeltaRef(Span<MarkId>),
    Size1PushedDeltaRef(Span<MarkId>),
    Size2PushedDeltaRef(Span<MarkId>),
    Size3PushedDeltaRef(Span<MarkId>),
    Size4PushedDeltaRef(Span<MarkId>),
    UnsizedRawDeltaRef(Span<MarkId>),
    Size1RawDeltaRef(Span<MarkId>),
    Size2RawDeltaRef(Span<MarkId>),
    Size3RawDeltaRef(Span<MarkId>),
    Size4RawDeltaRef(Span<MarkId>),
}

const _ASSERT_STORED_ASM_SECTION_MEM_SIZE: () = const {
    const_assert_mem_size::<StoredAsmSection>(12);
    assert!(std::mem::align_of::<StoredAsmSection>() == 4);
};

fn bytes_to_hold(offset: u32) -> RefSize {
    match offset {
        0x00..=0xff => RefSize::S1,
        0x100..=0xffff => RefSize::S2,
        0x10000..=0xffffff => RefSize::S3,
        0x1000000..=0xffffffff => RefSize::S4,
    }
}

impl From<StoredAsmSection> for AsmSection {
    fn from(value: StoredAsmSection) -> Self {
        match value {
            StoredAsmSection::Mark(id) => AsmSection::Mark(id),
            StoredAsmSection::Ops(span) => AsmSection::Ops(span),
            StoredAsmSection::Data(span) => AsmSection::Data(span),

            StoredAsmSection::DirectMarkRef(mark_ref) => AsmSection::MarkRef(AsmReference {
                mark_ref: MarkReference::Direct(mark_ref.id),
                set_size: mark_ref.set_size,
                pushed: mark_ref.pushed,
            }),
            StoredAsmSection::UnsizedPushedDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, true, None)
            }
            StoredAsmSection::Size1PushedDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, true, Some(RefSize::S1))
            }
            StoredAsmSection::Size2PushedDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, true, Some(RefSize::S2))
            }
            StoredAsmSection::Size3PushedDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, true, Some(RefSize::S3))
            }
            StoredAsmSection::Size4PushedDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, true, Some(RefSize::S4))
            }
            StoredAsmSection::UnsizedRawDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, false, None)
            }
            StoredAsmSection::Size1RawDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, false, Some(RefSize::S1))
            }
            StoredAsmSection::Size2RawDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, false, Some(RefSize::S2))
            }
            StoredAsmSection::Size3RawDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, false, Some(RefSize::S3))
            }
            StoredAsmSection::Size4RawDeltaRef(delta_span) => {
                AsmSection::delta_ref(delta_span, false, Some(RefSize::S4))
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum AsmSection {
    Mark(MarkId),
    Ops(Span<AsmBytesIdx>),
    Data(Span<AsmBytesIdx>),
    MarkRef(AsmReference),
}

impl AsmSection {
    fn delta_ref(delta_span: Span<MarkId>, pushed: bool, set_size: Option<RefSize>) -> Self {
        Self::MarkRef(AsmReference { mark_ref: MarkReference::Delta(delta_span), set_size, pushed })
    }

    fn min_compiled_size(&self) -> u32 {
        match self {
            AsmSection::Mark(_) => 0,
            AsmSection::Ops(bytes_span) | AsmSection::Data(bytes_span) => {
                bytes_span.end - bytes_span.start
            }
            AsmSection::MarkRef(mark_ref) => match (mark_ref.set_size, mark_ref.pushed) {
                (Some(set_size), true) => set_size as u32 + 1,
                (Some(set_size), false) => set_size as u32,
                (None, _) => 1,
            },
        }
    }

    fn compiled_size(&self, mark_map: &IndexVec<MarkId, u32>) -> Option<u32> {
        match self {
            AsmSection::Mark(_) => Some(0),
            AsmSection::Ops(bytes_span) | AsmSection::Data(bytes_span) => {
                Some(bytes_span.end - bytes_span.start)
            }
            AsmSection::MarkRef(mark_ref) => {
                let value = match mark_ref.mark_ref {
                    MarkReference::Direct(id) => mark_map[id],
                    MarkReference::Delta(span) => {
                        // The end mark may temporarily have an offset smaller than the start's if
                        // the start is updated *first* to a large value past end.
                        mark_map[span.end].saturating_sub(mark_map[span.start])
                    }
                };
                let ref_size = bytes_to_hold(value);
                match (mark_ref.set_size, mark_ref.pushed) {
                    (Some(set_size), _) if set_size < ref_size => None,
                    (Some(set_size), true) => Some(set_size as u32 + 1),
                    (Some(set_size), false) => Some(set_size as u32),
                    (None, true) if value == 0 => Some(1),
                    (None, true) => Some(1 + ref_size as u32),
                    (None, false) => Some(ref_size as u32),
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum MarkReference {
    Direct(MarkId),
    Delta(Span<MarkId>),
}

#[derive(Debug, Clone, Copy)]
pub struct AsmReference {
    pub mark_ref: MarkReference,
    pub set_size: Option<RefSize>,
    pub pushed: bool,
}

impl AsmReference {
    pub fn new_direct(id: MarkId) -> Self {
        Self { mark_ref: MarkReference::Direct(id), set_size: None, pushed: true }
    }

    pub fn new_delta(start: MarkId, end: MarkId) -> Self {
        Self { mark_ref: MarkReference::Delta(Span { start, end }), set_size: None, pushed: true }
    }

    pub fn pushed(mark_ref: MarkReference) -> Self {
        Self { mark_ref, set_size: None, pushed: true }
    }
}

#[derive(Debug, Clone)]
pub struct Assembler {
    bytes: IndexVec<AsmBytesIdx, u8>,
    sections: Vec<StoredAsmSection>,
}

#[derive(Debug, Clone)]
pub enum AssembleError {
    RefHasTooSmallSetSize(usize),
}

impl Assembler {
    pub fn with_capacity(bytes_capacity: usize, sections_capacity: usize) -> Self {
        Self {
            bytes: IndexVec::with_capacity(bytes_capacity),
            sections: Vec::with_capacity(sections_capacity),
        }
    }

    pub fn clear(&mut self) {
        self.bytes.clear();
        self.sections.clear();
    }

    fn iter_sections(&self) -> impl Iterator<Item = AsmSection> {
        self.sections.iter().map(|&section| section.into())
    }

    pub fn push_mark(&mut self, mark: MarkId) {
        self.sections.push(StoredAsmSection::Mark(mark))
    }

    pub fn push_op_byte(&mut self, byte: u8) {
        match self.sections.last_mut() {
            Some(StoredAsmSection::Ops(bytes_span)) => {
                assert!(bytes_span.end == self.bytes.len_idx(), "span out of sync");
                self.bytes.push(byte);
                bytes_span.end = self.bytes.len_idx();
            }
            _ => {
                let start = self.bytes.len_idx();
                self.bytes.push(byte);
                let end = self.bytes.len_idx();
                self.sections.push(StoredAsmSection::Ops(Span { start, end }));
            }
        }
    }

    #[track_caller]
    pub fn push_swap(&mut self, depth: u8) {
        let op = match depth {
            0 => panic!("noop swap0"),
            1 => op::SWAP1,
            2 => op::SWAP2,
            3 => op::SWAP3,
            4 => op::SWAP4,
            5 => op::SWAP5,
            6 => op::SWAP6,
            7 => op::SWAP7,
            8 => op::SWAP8,
            9 => op::SWAP9,
            10 => op::SWAP10,
            11 => op::SWAP11,
            12 => op::SWAP12,
            13 => op::SWAP13,
            14 => op::SWAP14,
            15 => op::SWAP15,
            16 => op::SWAP16,
            _ => panic!("unsupported swap with depth {depth}"),
        };
        self.push_op_byte(op);
    }

    #[track_caller]
    pub fn push_dup(&mut self, depth: u8) {
        let op = match depth {
            0 => op::DUP1,
            1 => op::DUP2,
            2 => op::DUP3,
            3 => op::DUP4,
            4 => op::DUP5,
            5 => op::DUP6,
            6 => op::DUP7,
            7 => op::DUP8,
            8 => op::DUP9,
            9 => op::DUP10,
            10 => op::DUP11,
            11 => op::DUP12,
            12 => op::DUP13,
            13 => op::DUP14,
            14 => op::DUP15,
            15 => op::DUP16,
            _ => panic!("unsupported dup with depth {depth}"),
        };
        self.push_op_byte(op);
    }

    #[track_caller]
    pub fn push_exchange(&mut self, n: u8, m: u8) {
        assert!(n != m, "noop exchange");
        match (n, m) {
            (0, m) => self.push_swap(m),
            (n, 0) => self.push_swap(n),
            (n, m) => {
                self.push_swap(n);
                self.push_swap(m);
                self.push_swap(n);
            }
        }
    }

    pub fn push_minimal_u256(&mut self, value: U256) {
        if value.is_zero() {
            self.push_op_byte(op::PUSH0);
            return;
        }

        let push_size = 32 - value.leading_zeros() / 8;
        assert!(push_size <= u8::MAX as usize);
        let push_op = op::PUSH1 + push_size as u8 - 1;
        assert!((op::PUSH1..=op::PUSH32).contains(&push_op));

        self.push_op_byte(push_op);
        for &byte in value.to_le_bytes::<32>()[..push_size].iter().rev() {
            self.push_op_byte(byte);
        }
    }

    pub fn push_minimal_u64(&mut self, value: u64) {
        self.push_minimal_u256(U256::from(value));
    }

    pub fn push_minimal_u32(&mut self, value: u32) {
        self.push_minimal_u256(U256::from(value));
    }

    pub fn push_data(&mut self, data: &[u8]) {
        match self.sections.last_mut() {
            Some(StoredAsmSection::Data(bytes_span)) => {
                debug_assert!(bytes_span.end == self.bytes.len_idx(), "span out of sync");
                self.bytes.extend_from_slice(data);
                bytes_span.end = self.bytes.len_idx();
            }
            _ => {
                let start = self.bytes.len_idx();
                self.bytes.extend_from_slice(data);
                let end = self.bytes.len_idx();
                self.sections.push(StoredAsmSection::Data(Span { start, end }));
            }
        }
    }

    pub fn push_reference(&mut self, asm_ref: AsmReference) {
        let delta_span = match asm_ref.mark_ref {
            MarkReference::Direct(id) => {
                self.sections.push(StoredAsmSection::DirectMarkRef(DirectMarkRef {
                    id,
                    set_size: asm_ref.set_size,
                    pushed: asm_ref.pushed,
                }));
                return;
            }
            MarkReference::Delta(span) => span,
        };
        let section = if asm_ref.pushed {
            match asm_ref.set_size {
                None => StoredAsmSection::UnsizedPushedDeltaRef(delta_span),
                Some(RefSize::S1) => StoredAsmSection::Size1PushedDeltaRef(delta_span),
                Some(RefSize::S2) => StoredAsmSection::Size2PushedDeltaRef(delta_span),
                Some(RefSize::S3) => StoredAsmSection::Size3PushedDeltaRef(delta_span),
                Some(RefSize::S4) => StoredAsmSection::Size4PushedDeltaRef(delta_span),
            }
        } else {
            match asm_ref.set_size {
                None => StoredAsmSection::UnsizedRawDeltaRef(delta_span),
                Some(RefSize::S1) => StoredAsmSection::Size1RawDeltaRef(delta_span),
                Some(RefSize::S2) => StoredAsmSection::Size2RawDeltaRef(delta_span),
                Some(RefSize::S3) => StoredAsmSection::Size3RawDeltaRef(delta_span),
                Some(RefSize::S4) => StoredAsmSection::Size4RawDeltaRef(delta_span),
            }
        };
        self.sections.push(section);
    }

    #[allow(unused)]
    fn eprint_mark_map(mark_to_offset: &IndexVec<MarkId, u32>) {
        eprint!("{{");
        for (id, offset) in mark_to_offset.enumerate_idx() {
            if id != MarkId::ZERO {
                eprint!(", ");
            }
            eprint!("{id}: {offset}");
        }
        eprint!("}}");
    }

    fn converge_mark_offsets(
        &self,
        mark_to_offset: &mut IndexVec<MarkId, u32>,
    ) -> Result<(), AssembleError> {
        let mut min_size = 0;
        for section in self.iter_sections() {
            if let AsmSection::Mark(id) = section {
                let size_for_id = id.get() as usize + 1;
                // Maintain `length == capacity`.
                let additional_to_reserve = size_for_id.saturating_sub(mark_to_offset.len());
                mark_to_offset.reserve(additional_to_reserve);
                let cap = mark_to_offset.capacity();
                mark_to_offset.resize(cap, 0);
                mark_to_offset[id] = min_size;
            }
            min_size += section.min_compiled_size();
        }

        for _ in 0..MAX_ASSEMBLER_CONVERGENCE_ITERS {
            let mut converged = true;
            let mut current_code_offset = 0;
            for (i, section) in self.iter_sections().enumerate() {
                if let AsmSection::Mark(id) = section {
                    let prev_offset = mark_to_offset[id];
                    if prev_offset != current_code_offset {
                        converged = false;
                        mark_to_offset[id] = current_code_offset;
                    }
                }

                current_code_offset += section
                    .compiled_size(mark_to_offset)
                    .ok_or(AssembleError::RefHasTooSmallSetSize(i))?;
            }

            if converged {
                return Ok(());
            }
        }

        unreachable!("assembly didn't converge")
    }

    pub fn assemble(
        &self,
        result: &mut Vec<u8>,
        mark_id_count_hint: Option<usize>,
    ) -> Result<IndexVec<MarkId, u32>, AssembleError> {
        let mut mark_to_offset: IndexVec<MarkId, u32> =
            index_vec![0; mark_id_count_hint.unwrap_or(ASSUMED_MARK_COUNT_WITHOUT_HINT)];

        self.converge_mark_offsets(&mut mark_to_offset)?;

        for stored_section in self.iter_sections() {
            match stored_section {
                AsmSection::Mark(_) => { /* Marks are sizeless */ }
                AsmSection::Ops(bytes) | AsmSection::Data(bytes) => {
                    result.extend_from_slice(&self.bytes[bytes.start..bytes.end]);
                }
                AsmSection::MarkRef(mark_ref) => {
                    let value = match mark_ref.mark_ref {
                        MarkReference::Direct(id) => mark_to_offset[id],
                        MarkReference::Delta(span) => {
                            mark_to_offset[span.end]
                                .checked_sub(mark_to_offset[span.start])
                                .expect("delta underflow after convergence: marks did not settle into valid positions")
                        }
                    };
                    if value == 0 && mark_ref.set_size.is_none() && mark_ref.pushed {
                        result.push(op::PUSH0);
                    } else {
                        let ref_size = bytes_to_hold(value);
                        assert!(
                            mark_ref.set_size.is_none_or(|set_size| set_size >= ref_size),
                            "reached code emission with invalid ref size"
                        );
                        let ref_size = mark_ref.set_size.unwrap_or(ref_size);
                        let value_bytes = value.to_le_bytes();
                        if mark_ref.pushed {
                            result.push(op::PUSH1 + ref_size as u8 - 1);
                        }
                        for i in (0..ref_size as usize).rev() {
                            result.push(value_bytes[i]);
                        }
                    }
                }
            }
        }

        Ok(mark_to_offset)
    }
}

#[cfg(test)]
mod tests {
    use alloy_primitives::hex;
    use plank_core::IncIterable;

    use super::*;

    #[test]
    fn test_basic_assembly() {
        let mut next_mark_id = MarkId::ZERO;

        let main = next_mark_id.get_and_inc();
        let mut asm = Assembler::with_capacity(256, 16);

        asm.push_reference(AsmReference::new_direct(main));
        asm.push_data(&[0x11; 253]);
        asm.push_mark(main);
        asm.push_op_byte(op::STOP);
        asm.push_op_byte(op::STOP);

        let mut result = Vec::with_capacity(300);
        asm.assemble(&mut result, Some(2)).unwrap();

        assert_eq!(
            hex::encode(result),
            "60ff111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111110000"
        );
    }

    #[test]
    fn test_pushes() {
        let mut asm = Assembler::with_capacity(256, 16);

        asm.push_minimal_u64(0);
        asm.push_minimal_u64(1);
        asm.push_minimal_u64(0xff);
        asm.push_minimal_u64(0x100);
        asm.push_minimal_u64(0x3103);
        asm.push_minimal_u64(0x10000);
        asm.push_minimal_u64(0x310ee);
        asm.push_minimal_u64(0xffffff);
        asm.push_minimal_u64(0x1000000);
        asm.push_minimal_u64(0x100000000000000);
        asm.push_minimal_u64(0xff0000000ccccccc);

        let mut result = Vec::with_capacity(300);
        asm.assemble(&mut result, Some(2)).unwrap();

        assert_eq!(
            hex::encode(result),
            "5f600160ff61010061310362010000620310ee62ffffff630100000067010000000000000067ff0000000ccccccc"
        );
    }
}
