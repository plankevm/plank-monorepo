use crate::{
    SourceId, SourceSpan,
    lexer::{Token, TokenIdx},
};

pub trait DiagnosticsContext {
    fn emit_lexer_error(
        &mut self,
        source_id: SourceId,
        token: Token,
        index: TokenIdx,
        src_span: SourceSpan,
    );

    fn emit_unexpected_token(
        &mut self,
        source_id: SourceId,
        found: Token,
        expected: &[Token],
        src_span: SourceSpan,
    );

    fn emit_missing_token(
        &mut self,
        source_id: SourceId,
        expected_main: Token,
        at_span: SourceSpan,
        expected: &[Token],
    );

    fn emit_unclosed_delimiter(
        &mut self,
        source_id: SourceId,
        opener: Token,
        open_span: SourceSpan,
        found_span: SourceSpan,
    );
}
