use sir_stack_scheduling_common::{
    CANONICAL_BLOCKS_HEADER, CanonicalBlockRow, RepresentativeSchedule, canonical_blocks_path,
    normalize_hash,
};
use std::{fs, path::Path};
use tempfile::NamedTempFile;

pub fn replace_best_schedule(
    database: &Path,
    requested_hash: &str,
    schedule: &RepresentativeSchedule,
    gas_cost: u64,
) -> Result<(), String> {
    let path = canonical_blocks_path(database);
    let canonical_hash = normalize_hash(requested_hash);
    let permissions = fs::metadata(&path)
        .map_err(|error| format!("failed to read '{}': {error}", path.display()))?
        .permissions();
    let mut reader = csv::Reader::from_path(&path)
        .map_err(|error| format!("failed to open '{}': {error}", path.display()))?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)
        .map_err(|error| format!("failed to create temporary database: {error}"))?;
    temporary
        .as_file_mut()
        .set_permissions(permissions)
        .map_err(|error| format!("failed to set database permissions: {error}"))?;

    let encoded_schedule = serde_json::to_string(schedule)
        .map_err(|error| format!("failed to encode submitted schedule: {error}"))?;
    let mut found = false;
    {
        let mut writer =
            csv::WriterBuilder::new().has_headers(false).from_writer(temporary.as_file_mut());
        writer
            .write_record(CANONICAL_BLOCKS_HEADER)
            .map_err(|error| format!("failed to write '{}': {error}", path.display()))?;
        for row in reader.deserialize::<CanonicalBlockRow>() {
            let mut row =
                row.map_err(|error| format!("failed to read '{}': {error}", path.display()))?;
            if row.canonical_hash == canonical_hash {
                if found {
                    return Err(format!(
                        "hash '{canonical_hash}' occurs more than once in '{}'",
                        path.display()
                    ));
                }
                if gas_cost >= row.best_gas_cost {
                    return Err(format!(
                        "submitted gas cost {gas_cost} does not improve the stored cost {}",
                        row.best_gas_cost
                    ));
                }
                row.best_schedule.clone_from(&encoded_schedule);
                row.best_gas_cost = gas_cost;
                found = true;
            }
            writer
                .serialize(row)
                .map_err(|error| format!("failed to write '{}': {error}", path.display()))?;
        }
        writer.flush().map_err(|error| format!("failed to flush '{}': {error}", path.display()))?;
    }
    if !found {
        return Err(format!("hash '{canonical_hash}' was not found in '{}'", path.display()));
    }
    drop(reader);
    temporary
        .as_file_mut()
        .sync_all()
        .map_err(|error| format!("failed to sync '{}': {error}", path.display()))?;
    temporary
        .persist(&path)
        .map_err(|error| format!("failed to replace '{}': {}", path.display(), error.error))?;
    Ok(())
}
