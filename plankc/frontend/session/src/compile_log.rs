use crate::SrcLoc;
use std::path::PathBuf;

use super::Session;

#[derive(Debug, Clone)]
pub struct CompileLog {
	pub loc: SrcLoc,
	pub msg: String,
}

impl CompileLog {
    pub fn format(&self, session: &Session) -> String {
        let source = session.get_source(self.loc.source);
        let last_two: PathBuf = source.path.components().rev().take(2).collect::<Vec<_>>()
            .into_iter().rev().collect();
        let (line, col) = session.offset_to_line_col(self.loc.source, self.loc.span.start);
        format!("[{}:{}:{}] {}", last_two.display(), line, col, self.msg)
    }
}
