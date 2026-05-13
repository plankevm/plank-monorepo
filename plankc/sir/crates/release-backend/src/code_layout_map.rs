//! ## Code Layout
//!
//! A given contract object's code will be laid out as follows:
//!
//! ```txt
//! @initcode_start
//!     <initcode/>
//! @init_only_data_start
//!     <data/>*
//! @runcode_start
//!     <runcode/>
//! @data_start
//!     <data/>*
//! @initcode_end
//! ```

use plank_core::Span;
use sir_assembler::MarkId;

#[derive(Debug, Clone, Copy)]
#[allow(unused)]
pub(crate) struct CodeLayoutMap {
    initcode_start: MarkId,
    init_only_data_start: MarkId,
    runcode_start: MarkId,
    data_start: MarkId,
    initcode_end: MarkId,
}

#[allow(unused)]
impl CodeLayoutMap {
    pub fn initcode(&self) -> Span<MarkId> {
        Span::new(self.initcode_start, self.initcode_end)
    }

    pub fn runcode(&self) -> Span<MarkId> {
        Span::new(self.runcode_start, self.initcode_end)
    }
}
