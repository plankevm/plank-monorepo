; Identifier conventions

; Assume all-caps names are constants
((identifier) @constant
 (#match? @constant "^[A-Z][A-Z0-9_]+$"))

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
  "@evm_add" "@evm_mul" "@evm_sub" "@evm_div" "@evm_sdiv" "@evm_mod" "@evm_smod"
  "@evm_addmod" "@evm_mulmod" "@evm_exp" "@evm_signextend"
  ; EVM Comparison & Bitwise Logic
  "@evm_lt" "@evm_gt" "@evm_slt" "@evm_sgt" "@evm_eq" "@evm_iszero"
  "@evm_and" "@evm_or" "@evm_xor" "@evm_not"
  "@evm_byte" "@evm_shl" "@evm_shr" "@evm_sar"
  ; EVM Keccak-256
  "@evm_keccak256"
  ; EVM Environment Information
  "@evm_address_this" "@evm_balance" "@evm_origin" "@evm_caller" "@evm_callvalue"
  "@evm_calldataload" "@evm_calldatasize" "@evm_calldatacopy"
  "@evm_codesize" "@evm_codecopy" "@evm_gasprice"
  "@evm_extcodesize" "@evm_extcodecopy" "@evm_returndatasize" "@evm_returndatacopy"
  "@evm_extcodehash" "@evm_gas"
  ; EVM Block Information
  "@evm_blockhash" "@evm_coinbase" "@evm_timestamp" "@evm_number" "@evm_difficulty"
  "@evm_gaslimit" "@evm_chainid" "@evm_selfbalance" "@evm_basefee"
  "@evm_blobhash" "@evm_blobbasefee"
  ; EVM State Manipulation
  "@evm_sload" "@evm_sstore" "@evm_tload" "@evm_tstore"
  ; EVM Logging Operations
  "@evm_log0" "@evm_log1" "@evm_log2" "@evm_log3" "@evm_log4"
  ; EVM System Calls
  "@evm_create" "@evm_create2" "@evm_call" "@evm_callcode" "@evm_delegatecall" "@evm_staticcall"
  "@evm_return" "@evm_stop" "@evm_revert" "@evm_invalid" "@evm_selfdestruct"
  ; IR Memory Primitives
  "@malloc_zeroed" "@malloc_uninit"
  ; Memory Manipulation
  "@mcopy"
  "@mload1" "@mload2" "@mload3" "@mload4" "@mload5" "@mload6" "@mload7" "@mload8"
  "@mload9" "@mload10" "@mload11" "@mload12" "@mload13" "@mload14" "@mload15" "@mload16"
  "@mload17" "@mload18" "@mload19" "@mload20" "@mload21" "@mload22" "@mload23" "@mload24"
  "@mload25" "@mload26" "@mload27" "@mload28" "@mload29" "@mload30" "@mload31" "@mload32"
  "@mstore1" "@mstore2" "@mstore3" "@mstore4" "@mstore5" "@mstore6" "@mstore7" "@mstore8"
  "@mstore9" "@mstore10" "@mstore11" "@mstore12" "@mstore13" "@mstore14" "@mstore15" "@mstore16"
  "@mstore17" "@mstore18" "@mstore19" "@mstore20" "@mstore21" "@mstore22" "@mstore23" "@mstore24"
  "@mstore25" "@mstore26" "@mstore27" "@mstore28" "@mstore29" "@mstore30" "@mstore31" "@mstore32"
  ; Bytecode Introspection
  "@runtime_start_offset" "@init_end_offset" "@runtime_length"
  ; Comptime Type Reflection
  "@is_struct" "@field_count" "@field_type" "@get_field" "@set_field"))

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
