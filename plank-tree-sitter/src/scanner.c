#include "tree_sitter/parser.h"
#include "tree_sitter/alloc.h"
#include "tree_sitter/array.h"

enum TokenType {
    BLOCK_COMMENT_CONTENT,
    ERROR_SENTINEL
};

typedef enum {
    Slash,
    Asterisk,
    Continuing,
} BlockCommentState;


void* tree_sitter_plank_external_scanner_create() {
    return NULL;
}

void tree_sitter_plank_external_scanner_destroy(void *payload) {
    // no state to destroy.
}

unsigned tree_sitter_plank_external_scanner_serialize(
  void *payload,
  char *buffer
) {
    return 0;
}

void tree_sitter_plank_external_scanner_deserialize(
  void *payload,
  const char *buffer,
  unsigned length
) { }

bool tree_sitter_plank_external_scanner_scan(
  void *payload,
  TSLexer *lexer,
  const bool *valid_symbols
) {
    // Recommended way of handling error state: https://tree-sitter.github.io/tree-sitter/creating-parsers/4-external-scanners.html#other-external-scanner-details
    if (valid_symbols[ERROR_SENTINEL]) {
        return false;
    }

    if (!valid_symbols[BLOCK_COMMENT_CONTENT]) {
        return false;
    }


    // We are only parsing content (`$._block_comment_content`):
    // `const BIG_NUMBER = 3749; /* commented stuff /* nested */ ok */`
    // Lexer gets started at  -----^

    BlockCommentState state = Continuing;
    uint32_t nesting_depth = 1;

    while (!lexer->eof(lexer) && nesting_depth != 0) {
        char current = (char)lexer->lookahead;

        switch (current) {
            case '*':
                // We want to mark the end as being right before '*/'. Tree sitter allows
                // calling `mark_end` many times, only last one counts.
                lexer->mark_end(lexer);
                if (state == Slash) {
                    state = Continuing;
                    nesting_depth += 1;
                } else {
                    state = Asterisk;
                }
                break;
            case '/':
                if (state == Asterisk) {
                    state = Continuing;
                    nesting_depth -= 1;
                } else {
                    state = Slash;
                }
                break;
        }

        lexer->advance(lexer, false);
    }

    // Still accept result even if we ended via EOF as it's useful while typing.

    lexer->result_symbol = BLOCK_COMMENT_CONTENT;
    return true;
}
