# Plank Grammar

```ebnf
program = decl*
decl = init | run | const_def | import

init = "init" block
run = "run" block
const_def = "const" IDENT (":" expr)? "=" expr ";"
import = "import" IDENT ("::" IDENT)* (suffix_import_all | suffix_import_as | suffix_import_group)? ";"
suffix_import_all = "::" "*"
suffix_import_as = "as" IDENT
suffix_import_group = "::" "{" import_group_item ("," import_group_item)* ","? "}"
import_group_item = IDENT ("as" IDENT)?

# Expressions
expr = "comptime"? block | if_expr | expr_no_block
expr_no_block =
    IDENT | BUILTIN_IDENT | literal | member
    | fn_call | fn_def | struct_def | struct_lit | tuple_type | tuple_lit
    | binary | unary | paren
binary = expr binary_op expr
unary = unary_op expr
paren = "(" expr ")"
fn_call = expr "(" comma_separated{expr}? ")"
member = expr "." IDENT
# Built-in attributes: cbytes.length

binary_op = "or" | "and"
          | "==" | "!=" | "<" | ">" | "<=" | ">="
          | "|" | "^" | "&" | "<<" | ">>"
          | "+" | "-" | "+%" | "-%"
          | "*" | "/" | "%" | "*%" | "+/" | "-/" | "</" | ">/"
unary_op = "-" | "!" | "~"

if_expr = "if" expr block ("else" "if" expr block)* ("else" block)?

block = "{" stmt* expr? "}"

# Statements
stmt =
    (expr_no_block | return | assign | let) ";"
    | if_expr ";"?
    | while

while = "inline"? "while" expr block
let = "let" "mut"? IDENT (":" expr)? "=" expr
return = "return" expr
assign = expr "=" expr

# Definitions
fn_def = "fn" "(" param_def_list? ")" expr block
param_def_list = comma_separated{"comptime"? IDENT ":" param_type}
param_type = expr | "$" IDENT

struct_def = "struct" expr? "{" comma_separated{IDENT ":" expr}? "}"
struct_lit = expr "{" comma_separated{IDENT ":" expr}? "}"
tuple_type = "tuple" "{" comma_separated{expr}? "}"
tuple_lit = "(" ")" | "(" expr "," comma_separated{expr}? ")"

# Literals
literal = bool_literal | hex_literal | bin_literal | dec_literal | bytes_literal
bool_literal = "true" | "false"
hex_literal = /-?0x[0-9A-Fa-f][0-9A-Fa-f_]*/
bin_literal = /-?0b[01][01_]*/
dec_literal = /-?[0-9][0-9_]*/

# Adjacent string/hex-string segments are concatenated into a single value:
# `"abc" "123" hex"01ab"` is one literal equal to `"abc123\x01\xab"`. This lets
# multi-line strings be split into segments and re-indented freely.
bytes_literal = (string_literal | hex_string_literal)+
string_literal = /"([^"\\\n\r]|escape)*"/
escape = "\n" | "\r" | "\t" | "\0" | "\\" | "\"" | /\\x[0-9A-Fa-f]{2}/
hex_string_literal = /hex"([0-9A-Fa-f]{2})*"/

# Helpers
comma_separated{p} = (p ("," p)* ","?)
BUILTIN_IDENT = /@[a-zA-Z_][a-zA-Z0-9_]*/
line_comment = /\/\/[^\n]*/
```
