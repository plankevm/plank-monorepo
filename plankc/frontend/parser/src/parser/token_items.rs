use crate::lexer::{Lexed, Token, TokenIdx};
use plank_core::Idx;
use plank_session::SourceSpan;

pub(crate) struct TokenItems<'a> {
    lexed: &'a Lexed,
    current: TokenIdx,
    fuel: u32,
}

impl<'a> TokenItems<'a> {
    const DEFAULT_FUEL: u32 = 1024;

    pub(crate) fn new(lexed: &'a Lexed) -> Self {
        TokenItems { lexed, current: TokenIdx::ZERO, fuel: Self::DEFAULT_FUEL }
    }

    #[allow(unused)]
    pub(super) fn fuel(&self) -> u32 {
        self.fuel
    }

    pub(crate) fn token_src_span(&self, ti: TokenIdx) -> SourceSpan {
        self.lexed.token_src_span(ti)
    }

    pub(crate) fn get_prev(&self) -> Option<(Token, SourceSpan)> {
        (self.current > TokenIdx::ZERO).then(|| self.lexed.get(self.current - 1))
    }

    pub(crate) fn current(&self) -> TokenIdx {
        self.current
    }

    pub(crate) fn peek(&mut self) -> (Token, SourceSpan) {
        self.fuel = self.fuel.checked_sub(1).expect("out of fuel: likely caused by infinite loop");
        self.lexed.get(self.current)
    }

    pub(super) fn next(&mut self) -> (Token, SourceSpan) {
        self.fuel = Self::DEFAULT_FUEL;
        let result = self.lexed.get(self.current);
        self.current += 1;
        result
    }
}
