use plank_hir::lower;
use plank_session::{Session, SourceId};
use plank_source::{
    ModuleResolver, ParsedProject, diagnostics, parse_project, source_fs::SourceFs,
};
use plank_values::ValueInterner;
use sir_passes::PassManager;
use std::path::{Path, PathBuf};

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

    pub fn emit_bytecode(
        &self,
        mir: &plank_mir::Mir,
        optimizations: Option<&str>,
        disp_needs_separators: bool,
        show_sir_in: bool,
        show_sir_last: bool,
    ) -> Vec<u8> {
        let mut program = plank_mir_lower::lower(mir, &self.values);
        if show_sir_in {
            if disp_needs_separators {
                eprintln!("\n");
                eprintln!("////////////////////////////////////////////////////////////////");
                eprintln!("//                           SIR IN                           //");
                eprintln!("////////////////////////////////////////////////////////////////");
            }
            eprintln!("{}", program);
        }
        let mut pass_manager = PassManager::new(&mut program);
        pass_manager.run_ssa_transform();
        if let Some(passes) = optimizations {
            pass_manager.run_optimizations(passes);
        }
        if show_sir_last {
            if disp_needs_separators {
                eprintln!("\n");
                eprintln!("////////////////////////////////////////////////////////////////");
                eprintln!("//                          SIR LAST                          //");
                eprintln!("////////////////////////////////////////////////////////////////");
            }
            eprintln!("{}", program);
        }
        let mut bytecode = Vec::with_capacity(0x6000);
        sir_debug_backend::ir_to_bytecode(&program, &mut bytecode);
        bytecode
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_source::source_fs::InMemoryFs;
    use plank_test_utils::{assert_diagnostics, dedent_preserve_indent};

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
