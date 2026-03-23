use plank_hir::lower;
use plank_session::Session;
use plank_source::{
    ModuleResolver, ParsedProject, diagnostics, parse_project, source_fs::SourceFs,
};
use plank_values::BigNumInterner;
use sir_passes::PassManager;
use std::path::{Path, PathBuf};

pub struct Driver<'a, F: SourceFs> {
    pub session: Session,
    pub big_nums: BigNumInterner,
    module_resolver: ModuleResolver,
    fs: &'a F,
}

impl<'a, F: SourceFs> Driver<'a, F> {
    pub fn new(fs: &'a F) -> Self {
        Self {
            session: Session::new(),
            big_nums: BigNumInterner::new(),
            module_resolver: ModuleResolver::default(),
            fs,
        }
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
        parse_project(entry_path, &self.module_resolver, &mut self.session, self.fs)
    }

    pub fn lower_hir(&mut self, project: &ParsedProject) -> plank_hir::Hir {
        lower(project, &mut self.big_nums, &mut self.session)
    }

    pub fn evaluate_hir(&self, hir: &plank_hir::Hir) -> plank_mir::Mir {
        plank_hir_eval::evaluate(hir)
    }

    pub fn emit_bytecode(
        &self,
        mir: &plank_mir::Mir,
        already_ssa: bool,
        optimizations: Option<&str>,
    ) -> Vec<u8> {
        let mut program = plank_mir_lower::lower(mir, &self.big_nums);
        let mut pass_manager = PassManager::new(&mut program);
        if already_ssa {
            pass_manager.run_legalize().expect("illegal IR pre-ssa");
        } else {
            pass_manager.run_ssa_transform();
        }
        if let Some(passes) = optimizations {
            pass_manager.run_optimizations(passes);
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

    #[test]
    fn duplicate_dep_emits_diagnostic() {
        let mut fs = InMemoryFs::new();
        fs.add_file("main.plk", "init {}\n".to_string());

        let mut driver = Driver::new(&fs);
        driver.register_module("m", PathBuf::from("/a"));
        driver.register_module("m", PathBuf::from("/b"));

        let rendered = driver.session.diagnostics()[0].render_plain(&driver.session);
        pretty_assertions::assert_str_eq!(
            rendered.trim(),
            "\
error: duplicate module 'm'
  |
  = help: each module name can only be registered once"
        );
    }

    #[test]
    fn missing_entry_file_emits_diagnostic() {
        let fs = InMemoryFs::new();
        let mut driver = Driver::new(&fs);
        let result = driver.load_project(Path::new("nonexistent.plk"));

        assert!(result.is_none());
        let rendered = driver.session.diagnostics()[0].render_plain(&driver.session);
        pretty_assertions::assert_str_eq!(
            rendered.trim(),
            "\
error: could not open entry file
  |
  = note: 'nonexistent.plk': file not found in InMemoryFs: nonexistent.plk"
        );
    }

    #[test]
    fn unknown_module_import_emits_diagnostic() {
        let mut fs = InMemoryFs::new();
        fs.add_file("main.plk", "import foo::bar::Baz;\ninit {}\n".to_string());

        let mut driver = Driver::new(&fs);
        driver.load_project(Path::new("main.plk"));

        let rendered = driver.session.diagnostics()[0].render_plain(&driver.session);
        pretty_assertions::assert_str_eq!(
            rendered.trim(),
            "\
error: unresolved import
 --> main.plk:1:8
  |
1 | import foo::bar::Baz;
  |        ^^^ unknown module 'foo'"
        );
    }

    #[test]
    fn imported_file_not_found_emits_diagnostic() {
        let mut fs = InMemoryFs::new();
        fs.add_file("main.plk", "import m::a::b::X;\ninit {}\n".to_string());

        let mut driver = Driver::new(&fs);
        driver.register_module("m", PathBuf::from("/lib"));
        driver.load_project(Path::new("main.plk"));

        let rendered = driver.session.diagnostics()[0].render_plain(&driver.session);
        pretty_assertions::assert_str_eq!(
            rendered.trim(),
            "\
error: could not open imported file
 --> main.plk:1:8
  |
1 | import m::a::b::X;
  |        ^^^^^^^ imported here
  |
  = note: '/lib/a/b.plk': file not found in InMemoryFs: /lib/a/b.plk"
        );
    }
}
