; Identifier conventions

; Assume all-caps names are constants
((identifier) @constant
 (#match? @constant "^[A-Z][A-Z\\d_]+$"))

; Type definitions (const assigned to a struct def)
(const_def
  (identifier) @type.definition
  (struct_def))

; Builtin types

((identifier) @type.builtin
 (#any-of? @type.builtin "void" "u256" "bool" "memptr" "type" "function" "never"))

; Builtin functions

((identifier) @function.builtin
 (#any-of? @function.builtin
  ; EVM Arithmetic
  "add" "mul" "sub" "raw_div" "raw_sdiv" "raw_mod" "raw_smod"
  "raw_addmod" "raw_mulmod" "exp" "signextend"
  ; EVM Comparison & Bitwise Logic
  "lt" "gt" "slt" "sgt" "eq" "iszero"
  "bitwise_and" "bitwise_or" "bitwise_xor" "bitwise_not"
  "byte" "shl" "shr" "sar"
  ; EVM Keccak-256
  "keccak256"
  ; EVM Environment Information
  "address_this" "balance" "origin" "caller" "callvalue"
  "calldataload" "calldatasize" "calldatacopy"
  "codesize" "codecopy" "gasprice"
  "extcodesize" "extcodecopy" "returndatasize" "returndatacopy"
  "extcodehash" "gas"
  ; EVM Block Information
  "blockhash" "coinbase" "timestamp" "number" "difficulty"
  "gaslimit" "chainid" "selfbalance" "basefee"
  "blobhash" "blobbasefee"
  ; EVM State Manipulation
  "sload" "sstore" "tload" "tstore"
  ; EVM Logging Operations
  "log0" "log1" "log2" "log3" "log4"
  ; EVM System Calls
  "create" "create2" "call" "callcode" "delegatecall" "staticcall"
  "evm_return" "evm_stop" "revert" "invalid" "selfdestruct"
  ; IR Memory Primitives
  "malloc_zeroed" "malloc_uninit"
  ; Memory Manipulation
  "mcopy"
  "mload1" "mload2" "mload3" "mload4" "mload5" "mload6" "mload7" "mload8"
  "mload9" "mload10" "mload11" "mload12" "mload13" "mload14" "mload15" "mload16"
  "mload17" "mload18" "mload19" "mload20" "mload21" "mload22" "mload23" "mload24"
  "mload25" "mload26" "mload27" "mload28" "mload29" "mload30" "mload31" "mload32"
  "mstore1" "mstore2" "mstore3" "mstore4" "mstore5" "mstore6" "mstore7" "mstore8"
  "mstore9" "mstore10" "mstore11" "mstore12" "mstore13" "mstore14" "mstore15" "mstore16"
  "mstore17" "mstore18" "mstore19" "mstore20" "mstore21" "mstore22" "mstore23" "mstore24"
  "mstore25" "mstore26" "mstore27" "mstore28" "mstore29" "mstore30" "mstore31" "mstore32"
  ; Bytecode Introspection
  "runtime_start_offset" "init_end_offset" "runtime_length"))

; Function calls

(fn_call
  fn: (identifier) @function (#set! priority 90))
(fn_call
  fn: (member
    (identifier) @function.method .))

; Member/field access

(member (identifier) @property .)

; Struct field definitions and initializations

(field_def
  name: (identifier) @property)
(field_init
  name: (identifier) @property)

; Parameters

(param_def
  name: (identifier) @variable.parameter)

; Comments

(line_comment) @comment
(block_comment) @comment

; Punctuation - brackets

"(" @punctuation.bracket
")" @punctuation.bracket
"{" @punctuation.bracket
"}" @punctuation.bracket

; Punctuation - delimiters

"::" @punctuation.delimiter
":" @punctuation.delimiter
"." @punctuation.delimiter
"," @punctuation.delimiter
";" @punctuation.delimiter

; Keywords

"as" @keyword
"comptime" @keyword
"const" @keyword
"else" @keyword
"fn" @keyword
"if" @keyword
"import" @keyword
"init" @keyword
"inline" @keyword
"let" @keyword
"mut" @keyword
"return" @keyword
"run" @keyword
"struct" @keyword
"while" @keyword

"or" @keyword
"and" @keyword

; Literals

(bool_literal) @boolean
(hex_literal) @number
(bin_literal) @number
(dec_literal) @number

; Operators

"=" @operator
"==" @operator
"!=" @operator
"<" @operator
">" @operator
"<=" @operator
">=" @operator
"|" @operator
"^" @operator
"&" @operator
"<<" @operator
">>" @operator
"+" @operator
"-" @operator
"+%" @operator
"-%" @operator
"*" @operator
"/" @operator
"%" @operator
"*%" @operator
"+/" @operator
"-/" @operator
"</" @operator
">/" @operator
"!" @operator
"~" @operator
