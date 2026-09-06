use crate::database::SourceBlock;
use std::fmt::Write;

pub fn source_blocks(source_blocks: &[SourceBlock]) -> String {
    let mut output = format!("source blocks ({}):", source_blocks.len());
    for source in source_blocks {
        write!(output, "\n  {}: bb{}", source.file, source.block_id).unwrap();
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_test_utils::dedent_preserve_indent;

    #[test]
    fn renders_source_file_and_block_mappings() {
        let sources = [
            SourceBlock { file: "first.sir".to_owned(), block_id: 4 },
            SourceBlock { file: "nested/second.sir".to_owned(), block_id: 17 },
        ];
        let expected = dedent_preserve_indent(
            r#"
            source blocks (2):
              first.sir: bb4
              nested/second.sir: bb17
            "#,
        );

        assert_eq!(source_blocks(&sources), expected);
    }
}
