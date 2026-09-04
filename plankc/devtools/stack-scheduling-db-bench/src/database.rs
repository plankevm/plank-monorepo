use sir_stack_scheduling_common::{
    CANONICAL_BLOCKS_HEADER, CanonicalBlockRow, canonical_blocks_path,
};
use std::{fs, path::PathBuf};
use tempfile::NamedTempFile;

pub struct Database {
    path: PathBuf,
    pub rows: Box<[CanonicalBlockRow]>,
}

impl Database {
    pub fn load(path: PathBuf) -> Result<Self, String> {
        let path = canonical_blocks_path(&path);
        let mut reader = csv::Reader::from_path(&path)
            .map_err(|error| format!("failed to open '{}': {error}", path.display()))?;
        let rows = reader
            .deserialize::<CanonicalBlockRow>()
            .collect::<Result<Box<[_]>, _>>()
            .map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
        if rows.is_empty() {
            return Err(format!("'{}' contains no canonical blocks", path.display()));
        }
        Ok(Self { path, rows })
    }

    pub fn save(&self) -> Result<(), String> {
        let parent = self.path.parent().expect("canonical database path has no parent");
        let permissions = fs::metadata(&self.path)
            .map_err(|error| format!("failed to read '{}': {error}", self.path.display()))?
            .permissions();
        let mut temporary = NamedTempFile::new_in(parent)
            .map_err(|error| format!("failed to create temporary database: {error}"))?;
        temporary
            .as_file_mut()
            .set_permissions(permissions)
            .map_err(|error| format!("failed to set database permissions: {error}"))?;
        {
            let mut writer =
                csv::WriterBuilder::new().has_headers(false).from_writer(temporary.as_file_mut());
            writer
                .write_record(CANONICAL_BLOCKS_HEADER)
                .map_err(|error| format!("failed to write '{}': {error}", self.path.display()))?;
            for row in &self.rows {
                writer.serialize(row).map_err(|error| {
                    format!("failed to write '{}': {error}", self.path.display())
                })?;
            }
            writer
                .flush()
                .map_err(|error| format!("failed to flush '{}': {error}", self.path.display()))?;
        }
        temporary
            .as_file_mut()
            .sync_all()
            .map_err(|error| format!("failed to sync '{}': {error}", self.path.display()))?;
        temporary.persist(&self.path).map_err(|error| {
            format!("failed to replace '{}': {}", self.path.display(), error.error)
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plank_test_utils::dedent_preserve_indent;
    use sir_stack_scheduling_common::CANONICAL_BLOCKS_FILE_NAME;

    #[test]
    fn rewrites_the_complete_canonical_database() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join(CANONICAL_BLOCKS_FILE_NAME);
        std::fs::write(
            &path,
            "canonical_hash,canonical_graph,best_schedule,best_gas_cost\nssb1:test,{},[],3\n",
        )
        .unwrap();
        let mut database = Database::load(temporary.path().to_owned()).unwrap();
        database.rows[0].best_schedule = "[{\"kind\":\"pop\"}]".to_owned();
        database.rows[0].best_gas_cost = 0;

        database.save().unwrap();

        let expected = dedent_preserve_indent(
            r#"
            canonical_hash,canonical_graph,best_schedule,best_gas_cost
            ssb1:test,{},"[{""kind"":""pop""}]",0
            "#,
        ) + "\n";
        assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
    }
}
