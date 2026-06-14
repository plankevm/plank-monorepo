#include "tree_sitter/parser.h"
#include "tree_sitter/alloc.h"
#include "tree_sitter/array.h"

enum TokenType {
    BLOCK_COMMENT_CONTENT,
    STRING_LITERAL_END,
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

static void scan_string_literal_end(TSLexer *lexer) {
    lexer->result_symbol = STRING_LITERAL_END;

    while (!lexer->eof(lexer)) {
        switch (lexer->lookahead) {
            case '"':
                lexer->advance(lexer, false);
                lexer->mark_end(lexer);
                return;
            case '\\':
                lexer->advance(lexer, false);
                if (!lexer->eof(lexer)) {
                    lexer->advance(lexer, false);
                }
                break;
            default:
                lexer->advance(lexer, false);
                break;
        }
    }
}

static void scan_block_comment_content(TSLexer *lexer) {
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
            default:
                state = Continuing;
        }

        lexer->advance(lexer, false);
    }

    // Still accept result even if we ended via EOF as it's useful while typing.
    lexer->result_symbol = BLOCK_COMMENT_CONTENT;
    return;

}

bool tree_sitter_plank_external_scanner_scan(
  void *payload,
  TSLexer *lexer,
  const bool *valid_symbols
) {
    // Recommended way of handling error state: https://tree-sitter.github.io/tree-sitter/creating-parsers/4-external-scanners.html#other-external-scanner-details
    if (valid_symbols[ERROR_SENTINEL]) {
        return false;
    }

    // `$._string_literal_end` only valid when expected
    if (valid_symbols[STRING_LITERAL_END]) {
        scan_string_literal_end(lexer);
        return true;
    }

    // `$._block_comment_content` only valid when expected
    if (valid_symbols[BLOCK_COMMENT_CONTENT]) {
        scan_block_comment_content(lexer);
        return true;
    }

    return false;
}
