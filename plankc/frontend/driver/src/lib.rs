use plank_hir::lower;
use plank_session::{Session, SourceId};
use plank_source::{
    ModuleResolver, ParsedProject, diagnostics, parse_project, source_fs::SourceFs,
};
use plank_values::ValueInterner;
use sir_passes::{PassManager, parse_optimizations_string};
use std::{
    fmt::Display,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum BackendKind {
    #[default]
    SirDebug,
    SirRelease,
    Sona,
}

pub struct Driver<'a, F: SourceFs> {
    pub session: Session,
    pub values: ValueInterner,
    module_resolver: ModuleResolver,
    fs: &'a F,
    std_root: Option<PathBuf>,
}

impl<'a, F: SourceFs> Driver<'a, F> {
    pub fn new(fs: &'a F) -> Self {
        Self {
            session: Session::new(),
            values: ValueInterner::new(),
            module_resolver: ModuleResolver::default(),
            fs,
            std_root: None,
        }
    }

    pub fn register_std(&mut self, root: PathBuf) {
        let name_id = self.session.intern("std");
        if self.module_resolver.register(name_id, root.clone()).is_err() {
            diagnostics::error_duplicate_module(&mut self.session, name_id);
        }
        self.std_root = Some(root);
    }

    pub fn register_module(&mut self, name: &str, root: PathBuf) {
        let name_id = self.session.intern(name);
        if self.module_resolver.register(name_id, root).is_err() {
            diagnostics::error_duplicate_module(&mut self.session, name_id);
        }
    }

    pub fn render_diagnostics_and_exit(&self) -> ! {
        for diagnostic in self.session.diagnostics() {
            anstream::eprintln!("{}\n", diagnostic.render_styled(&self.session));
        }
        std::process::exit(1)
    }

    pub fn load_project(&mut self, entry_path: &Path) -> Option<ParsedProject> {
        let core_ops_path = self.std_root.as_ref().map(|root| root.join("core_ops.plk"));
        parse_project(
            entry_path,
            core_ops_path.as_deref(),
            &self.module_resolver,
            &mut self.session,
            self.fs,
        )
    }

    pub fn lower_hir(&mut self, project: &ParsedProject) -> plank_hir::Hir {
        lower(project, &mut self.values, &mut self.session)
    }

    pub fn evaluate_hir(
        &mut self,
        hir: &plank_hir::Hir,
        core_ops_source: Option<SourceId>,
    ) -> plank_mir::Mir {
        plank_hir_eval::evaluate(hir, core_ops_source, &mut self.values, &mut self.session)
    }

    pub fn emit_bytecode_with_backend(
        &self,
        mir: &plank_mir::Mir,
        optimizations: Option<&str>,
        disp_needs_separators: bool,
        show_sir_in: bool,
        show_sir_last: bool,
        backend: BackendKind,
    ) -> Result<Vec<u8>, String> {
        let is_sir_debug = match backend {
            BackendKind::Sona => {
                return self.emit_sona_bytecode(
                    mir,
                    optimizations,
                    disp_needs_separators,
                    show_sir_in,
                    show_sir_last,
                );
            }
            BackendKind::SirDebug => true,
            BackendKind::SirRelease => false,
        };
        self.emit_sir_bytecode(
            mir,
            optimizations,
            disp_needs_separators,
            show_sir_in,
            show_sir_last,
            is_sir_debug,
        )
    }

    fn emit_sir_bytecode(
        &self,
        mir: &plank_mir::Mir,
        optimizations: Option<&str>,
        disp_needs_separators: bool,
        show_sir_in: bool,
        show_sir_last: bool,
        is_sir_debug_backend: bool,
    ) -> Result<Vec<u8>, String> {
        let mut program = plank_mir_lower::lower(mir, &self.values, &self.session);
        if show_sir_in {
            print_backend_ir("SIR IN", disp_needs_separators, &program);
        }
        let mut pass_manager = PassManager::new(&mut program);
        pass_manager.run_ssa_transform();
        if let Some(passes) = optimizations {
            parse_optimizations_string(passes)?;
            pass_manager.run_optimizations(passes);
        }
        let analyses = pass_manager.into_store();
        if show_sir_last {
            print_backend_ir("SIR LAST", disp_needs_separators, &program);
        }

        let mut bytecode = Vec::with_capacity(0x6000);
        if is_sir_debug_backend {
            sir_debug_backend::ir_to_bytecode(&program, &mut bytecode);
        } else {
            sir_release_backend::ir_to_bytecode(&program, &analyses, &mut bytecode);
        }
        Ok(bytecode)
    }

    fn emit_sona_bytecode(
        &self,
        mir: &plank_mir::Mir,
        optimizations: Option<&str>,
        disp_needs_separators: bool,
        show_sir_in: bool,
        show_sir_last: bool,
    ) -> Result<Vec<u8>, String> {
        let opt_level = parse_sona_opt_level(optimizations)?;
        if show_sir_in {
            print_backend_ir(
                "SONA IR",
                disp_needs_separators,
                plank_mir_lower_sona::emit_ir(
                    mir,
                    &self.values,
                    &self.session,
                    plank_mir_lower_sona::OptLevel::O0,
                )
                .map_err(|err| err.to_string())?,
            );
        }
        if show_sir_last {
            print_backend_ir(
                "SONA IR OPT",
                disp_needs_separators,
                plank_mir_lower_sona::emit_ir(mir, &self.values, &self.session, opt_level)
                    .map_err(|err| err.to_string())?,
            );
        }
        plank_mir_lower_sona::emit_bytecode(mir, &self.values, &self.session, opt_level)
            .map_err(|err| err.to_string())
    }
}

fn parse_sona_opt_level(
    optimizations: Option<&str>,
) -> Result<plank_mir_lower_sona::OptLevel, String> {
    let Some(optimizations) = optimizations else {
        return Ok(plank_mir_lower_sona::OptLevel::O0);
    };

    match optimizations.to_ascii_lowercase().as_str() {
        "0" | "o0" => Ok(plank_mir_lower_sona::OptLevel::O0),
        "1" | "o1" => Ok(plank_mir_lower_sona::OptLevel::O1),
        "s" | "os" => Ok(plank_mir_lower_sona::OptLevel::Os),
        "2" | "o2" => Ok(plank_mir_lower_sona::OptLevel::O2),
        _ => Err(format!(
            "invalid Sona optimization level '{optimizations}', valid levels: O0, O1, Os, O2"
        )),
    }
}

fn print_backend_ir(title: &str, disp_needs_separators: bool, ir: impl Display) {
    if disp_needs_separators {
        eprintln!("\n");
        eprintln!("////////////////////////////////////////////////////////////////");
        eprintln!("//{title:^60}//");
        eprintln!("////////////////////////////////////////////////////////////////");
    }
    eprintln!("{ir}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_source::source_fs::InMemoryFs;
    use plank_test_utils::{assert_diagnostics, dedent_preserve_indent};

    #[test]
    fn parse_sona_opt_level_accepts_explicit_levels() {
        assert_eq!(parse_sona_opt_level(None).unwrap(), plank_mir_lower_sona::OptLevel::O0);
        assert_eq!(parse_sona_opt_level(Some("0")).unwrap(), plank_mir_lower_sona::OptLevel::O0);
        assert_eq!(parse_sona_opt_level(Some("O0")).unwrap(), plank_mir_lower_sona::OptLevel::O0);
        assert_eq!(parse_sona_opt_level(Some("O1")).unwrap(), plank_mir_lower_sona::OptLevel::O1);
        assert_eq!(parse_sona_opt_level(Some("Os")).unwrap(), plank_mir_lower_sona::OptLevel::Os);
        assert_eq!(parse_sona_opt_level(Some("O2")).unwrap(), plank_mir_lower_sona::OptLevel::O2);
    }

    #[test]
    fn parse_sona_opt_level_rejects_sir_passes() {
        assert_eq!(
            parse_sona_opt_level(Some("csud")).unwrap_err(),
            "invalid Sona optimization level 'csud', valid levels: O0, O1, Os, O2"
        );
    }

    #[test]
    fn duplicate_dep_emits_diagnostic() {
        let mut fs = InMemoryFs::new();
        fs.add_file("main.plk", "init {}\n".to_string());

        let mut driver = Driver::new(&fs);
        driver.register_module("m", PathBuf::from("/a"));
        driver.register_module("m", PathBuf::from("/b"));

        assert_diagnostics(
            driver.session.diagnostics(),
            &driver.session,
            &[r#"
            error: duplicate module 'm'
              |
              = help: each module name can only be registered once
            "#],
        );
    }

    #[test]
    fn missing_entry_file_emits_diagnostic() {
        let fs = InMemoryFs::new();
        let mut driver = Driver::new(&fs);
        let result = driver.load_project(Path::new("nonexistent.plk"));
        assert!(result.is_none());

        assert_diagnostics(
            driver.session.diagnostics(),
            &driver.session,
            &[r#"
            error: could not open entry file
              |
              = note: 'nonexistent.plk': file not found in InMemoryFs: nonexistent.plk
            "#],
        );
    }

    #[test]
    fn unknown_module_import_emits_diagnostic() {
        let mut fs = InMemoryFs::new();
        fs.add_file("main.plk", "import foo::bar::Baz;\ninit {}\n".to_string());

        let mut driver = Driver::new(&fs);
        driver.load_project(Path::new("main.plk"));

        assert_diagnostics(
            driver.session.diagnostics(),
            &driver.session,
            &[r#"
            error: unresolved import
             --> main.plk:1:8
              |
            1 | import foo::bar::Baz;
              |        ^^^ unknown module 'foo'
            "#],
        );
    }

    #[test]
    fn test_unknown_std_module_import_emits_diagnostic_with_help() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            "main.plk",
            dedent_preserve_indent(
                r#"
                import std::math::max;
                init {}
                "#,
            )
            .to_string(),
        );

        let mut driver = Driver::new(&fs);
        driver.load_project(Path::new("main.plk"));

        assert_diagnostics(
            driver.session.diagnostics(),
            &driver.session,
            &[r#"
            error: unresolved import
             --> main.plk:1:8
              |
            1 | import std::math::max;
              |        ^^^ unknown module 'std'
              |
              = help: the 'std' module is included with plankup, the Plank installer
              = note: see https://github.com/plankevm/plank-monorepo for installation instructions
            "#],
        );
    }

    #[test]
    fn imported_file_not_found_emits_diagnostic() {
        let mut fs = InMemoryFs::new();
        fs.add_file("main.plk", "import m::a::b::X;\ninit {}\n".to_string());

        let mut driver = Driver::new(&fs);
        driver.register_module("m", PathBuf::from("/lib"));
        driver.load_project(Path::new("main.plk"));

        assert_diagnostics(
            driver.session.diagnostics(),
            &driver.session,
            &[r#"
            error: could not open imported file
             --> main.plk:1:8
              |
            1 | import m::a::b::X;
              |        ^^^^^^^ imported here
              |
              = note: '/lib/a/b.plk': file not found in InMemoryFs: /lib/a/b.plk
            "#],
        );
    }
}
