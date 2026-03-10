use crate::{Idx, Span, newtype_index};

newtype_index! {
    pub struct SourceId;
    pub struct SourceByteOffset;
}

impl SourceId {
    pub const ROOT: SourceId = SourceId::ZERO;
}

pub type SourceSpan = Span<SourceByteOffset>;
