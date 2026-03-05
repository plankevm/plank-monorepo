use crate::{
    lexer::{SourceSpan, Token, TokenIdx},
    source::SourceId,
};

pub trait DiagnosticsContext {
    fn emit_lexer_error(&mut self, token: Token, index: TokenIdx, src_span: SourceSpan);

    fn emit_unexpected_token(&mut self, found: Token, expected: &[Token], src_span: SourceSpan);
    fn emit_missing_token(&mut self, expected: Token, at_span: SourceSpan);
    fn emit_unclosed_delimiter(
        &mut self,
        opener: Token,
        open_span: SourceSpan,
        found_span: SourceSpan,
    );
    fn finish_source(&mut self, source_id: SourceId);
}
