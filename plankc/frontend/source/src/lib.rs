pub mod module;
pub mod project;
pub mod source_fs;

pub use module::ModuleResolver;
pub use project::{ParsedProject, ParsedSource, parse_project};
pub use source_fs::SourceFs;

pub const FILE_EXTENSION: &str = "plk";

#[cfg(test)]
mod tests;
