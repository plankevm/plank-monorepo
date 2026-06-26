use crate::SrcLoc;
use std::fmt;

#[derive(Debug, Clone)]
pub struct CompileLog {
    pub loc: SrcLoc,
    pub msg: String,
}

impl fmt::Display for CompileLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.msg)
    }
}
