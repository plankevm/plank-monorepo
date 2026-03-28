# Frontend Architecture

The architecture for the frontend of the compiler, to be usable for both the
codegeneration CLI and eventually also LSP server.

1. Lexing & Parser (`plankc/frontend/parser`): input source files are parsed into CSTs
2. HIR Gen (`plankc/frontend/hir`): Performs simultaenous analysis and lowering from
   the CST to untyped HIR with the help of AST wrappers.
3. Evaluator (`plankc/frontend/hir-eval`):
    Evalutes the HIR to resolve compile-time computations and monomorphize
    comptime functions. Responsible for full type checkingj
4. MIR => Non-SSA SIR final lowering (`plankc/frontend/mir-lower`):
    - MIR structs are flattened to locals
    - control flow constructs transformed into control flow graphs and basic
      blocks

Once lowered into SIR we've reached the middle-end and are no longer in the
frontend.
