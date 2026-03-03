# Frontend Architecture

The architecture for the frontend of the compiler, to be usable for both the
codegeneration CLI and eventually also LSP server.

1. Lexing & Parser (`senseic/frontend/parser`): input source files are parsed into CSTs
2. HIR Gen (`senseic/frontend/hir`): Performs simultaenous analysis and lowering from
   the CST to untyped HIR with the help of AST wrappers.
3. Evaluator (`senseic/frontend/hir-eval`):
    Evalutes the HIR to resolve compile-time computations and monomorphize
    comptime functions. Responsible for full type checkingj
4. MIR => SIR final lowering (not yet implemented, update doc once done):
    - MIR structs are flattened to locals
    - control flow constructs transformed into BBs & CFGs
    - SSA Transform invoked to turn into SSA

Once lowered into SIR we've reached the middle-end and are no longer in the
frontend.
