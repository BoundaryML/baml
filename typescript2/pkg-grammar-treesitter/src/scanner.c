#include "tree_sitter/parser.h"

enum TokenType { COMPANION_AT };

void *tree_sitter_baml_external_scanner_create(void) { return NULL; }
void tree_sitter_baml_external_scanner_destroy(void *p) {}
unsigned tree_sitter_baml_external_scanner_serialize(void *p, char *buf) { return 0; }
void tree_sitter_baml_external_scanner_deserialize(void *p, const char *b, unsigned n) {}

bool tree_sitter_baml_external_scanner_scan(void *p, TSLexer *lexer, const bool *valid_symbols) {
    if (!valid_symbols[COMPANION_AT]) return false;
    while (lexer->lookahead == ' ' || lexer->lookahead == '\t') lexer->advance(lexer, true);
    if (lexer->lookahead != '@') return false;
    lexer->advance(lexer, false);
    lexer->result_symbol = COMPANION_AT;
    return true;
}
