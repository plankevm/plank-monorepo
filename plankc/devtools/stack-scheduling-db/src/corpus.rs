use std::{
    fs,
    path::{Path, PathBuf},
};

pub struct Corpus {
    input: PathBuf,
    files: Box<[PathBuf]>,
}

pub struct CorpusEntry {
    pub display_path: PathBuf,
    pub source: String,
}

impl Corpus {
    pub fn load(input: PathBuf) -> Self {
        assert!(input.exists(), "input '{}' does not exist", input.display());
        let mut files = Vec::new();
        discover_sir_files(&input, &mut files);
        files.sort();
        assert!(!files.is_empty(), "no SIR files found under {}", input.display());
        Self { input, files: files.into_boxed_slice() }
    }

    pub fn len(&self) -> usize {
        self.files.len()
    }

    pub fn entries(&self) -> impl Iterator<Item = CorpusEntry> + '_ {
        self.files.iter().map(|path| {
            let source = fs::read_to_string(path)
                .unwrap_or_else(|error| panic!("failed to read '{}': {error}", path.display()));
            let display_path = if self.input.is_dir() {
                path.strip_prefix(&self.input).expect("corpus file escaped its root").to_owned()
            } else {
                path.clone()
            };
            CorpusEntry { display_path, source }
        })
    }
}

fn discover_sir_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        let name = path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        if path.extension().and_then(|extension| extension.to_str()) == Some("sir")
            && !name.starts_with("._")
        {
            files.push(path.to_owned());
        }
        return;
    }

    if path.file_name().and_then(|name| name.to_str()) == Some("__MACOSX") {
        return;
    }

    for entry in fs::read_dir(path)
        .unwrap_or_else(|error| panic!("failed to read directory '{}': {error}", path.display()))
    {
        discover_sir_files(&entry.unwrap().path(), files);
    }
}
