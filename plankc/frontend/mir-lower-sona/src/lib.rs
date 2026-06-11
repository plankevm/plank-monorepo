mod function;
mod module;

#[cfg(test)]
mod tests;

use plank_mir::Mir;
use plank_session::Session;
use plank_values::ValueInterner;
use sonatina_codegen::EvmCompile;
pub use sonatina_codegen::OptLevel;
use sonatina_ir::{Module, ir_writer::ModuleWriter, isa::evm::Evm, object::SectionName};
use sonatina_triple::{Architecture, EvmVersion, OperatingSystem, TargetTriple, Vendor};
use std::fmt;

pub(crate) const CONTRACT_OBJECT: &str = "Contract";
pub(crate) const INIT_SECTION: &str = "init";
pub(crate) const RUNTIME_SECTION: &str = "runtime";

#[derive(Debug, thiserror::Error)]
pub enum LowerError {
    #[error("Sonatina builder error: {0}")]
    Builder(String),
    #[error("Sonatina object compilation failed: {0}")]
    ObjectCompile(String),
}

pub fn lower(
    isa: &Evm,
    mir: &Mir,
    values: &ValueInterner,
    session: &Session,
) -> Result<Module, LowerError> {
    module::lower(isa, mir, values, session)
}

fn default_isa() -> Evm {
    Evm::new(TargetTriple::new(
        Architecture::Evm,
        Vendor::Ethereum,
        OperatingSystem::Evm(EvmVersion::Osaka),
    ))
}

pub fn emit_ir(
    mir: &Mir,
    values: &ValueInterner,
    session: &Session,
    opt_level: OptLevel,
) -> Result<String, LowerError> {
    let isa = default_isa();
    let module = lower(&isa, mir, values, session)?;
    let mut compile = EvmCompile::new(module).with_opt_level(opt_level);
    Ok(ModuleWriter::new(compile.optimize()).dump_string())
}

pub fn emit_bytecode(
    mir: &Mir,
    values: &ValueInterner,
    session: &Session,
    opt_level: OptLevel,
) -> Result<Vec<u8>, LowerError> {
    let isa = default_isa();
    let module = lower(&isa, mir, values, session)?;
    let artifact = EvmCompile::new(module)
        .with_opt_level(opt_level)
        .compile()
        .map_err(|errors| object_compile_error(&errors))?
        .into_iter()
        .next()
        .ok_or_else(|| LowerError::ObjectCompile("no objects compiled".into()))?;
    let init = SectionName::from(INIT_SECTION);
    artifact
        .sections
        .get(&init)
        .map(|section| section.bytes.clone())
        .ok_or_else(|| LowerError::ObjectCompile("compiled object has no init section".into()))
}

pub(crate) fn builder_error(error: impl fmt::Display) -> LowerError {
    LowerError::Builder(error.to_string())
}

fn object_compile_error(errors: &[impl fmt::Debug]) -> LowerError {
    LowerError::ObjectCompile(
        errors.iter().map(|err| format!("{err:?}")).collect::<Vec<_>>().join("; "),
    )
}
