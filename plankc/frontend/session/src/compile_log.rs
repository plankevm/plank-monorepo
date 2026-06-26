use crate::SrcLoc;

#[derive(Debug, Clone)]
pub struct CompileLog {
    pub loc: SrcLoc,
    pub msg: String,
}

impl CompileLog {
    pub fn format(&self) -> String {
        self.msg.clone()
    }
}
