use std::path::Path;

use devenv_core::{
    ArchiveExtractor, ArchiveType, CoreError, CoreResult, ExtractionManifest,
    validate_archive_manifest,
};

#[derive(Debug, Clone, Default)]
pub struct ManifestArchiveExtractor;

impl ArchiveExtractor for ManifestArchiveExtractor {
    fn extract(
        &mut self,
        archive_path: &Path,
        destination: &Path,
        archive_type: ArchiveType,
    ) -> CoreResult<ExtractionManifest> {
        if archive_type == ArchiveType::PlainFile {
            return extract_plain_file(archive_path, destination);
        }

        let contents = std::fs::read_to_string(archive_path).map_err(|error| {
            CoreError::message(format!(
                "failed to read manifest archive `{}`: {error}",
                archive_path.display()
            ))
        })?;
        let entries = contents
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(manifest_entry_path)
            .collect::<Vec<_>>();
        let manifest = ExtractionManifest::new(entries.iter().map(|(path, _)| path.clone()));
        validate_archive_manifest(&manifest)?;

        std::fs::create_dir_all(destination).map_err(|error| {
            CoreError::message(format!(
                "failed to create extraction destination `{}`: {error}",
                destination.display()
            ))
        })?;

        for (entry, content) in entries {
            let output = destination.join(&entry);
            if entry.to_string_lossy().ends_with('/') {
                std::fs::create_dir_all(&output).map_err(|error| {
                    CoreError::message(format!(
                        "failed to create extracted directory `{}`: {error}",
                        output.display()
                    ))
                })?;
                continue;
            }

            if let Some(parent) = output.parent() {
                std::fs::create_dir_all(parent).map_err(|error| {
                    CoreError::message(format!(
                        "failed to create extracted file parent `{}`: {error}",
                        parent.display()
                    ))
                })?;
            }
            std::fs::write(&output, content).map_err(|error| {
                CoreError::message(format!(
                    "failed to write extracted file `{}`: {error}",
                    output.display()
                ))
            })?;
        }

        Ok(manifest)
    }
}

fn extract_plain_file(archive_path: &Path, destination: &Path) -> CoreResult<ExtractionManifest> {
    let filename = archive_path.file_name().ok_or_else(|| {
        CoreError::message(format!(
            "failed to extract plain file `{}`: missing file name",
            archive_path.display()
        ))
    })?;
    let output = destination.join(filename);

    std::fs::create_dir_all(destination).map_err(|error| {
        CoreError::message(format!(
            "failed to create extraction destination `{}`: {error}",
            destination.display()
        ))
    })?;
    std::fs::copy(archive_path, &output).map_err(|error| {
        CoreError::message(format!(
            "failed to copy plain file `{}` to `{}`: {error}",
            archive_path.display(),
            output.display()
        ))
    })?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = std::fs::metadata(&output)
            .map_err(|error| {
                CoreError::message(format!(
                    "failed to read extracted plain file `{}` metadata: {error}",
                    output.display()
                ))
            })?
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&output, permissions).map_err(|error| {
            CoreError::message(format!(
                "failed to mark extracted plain file `{}` executable: {error}",
                output.display()
            ))
        })?;
    }

    Ok(ExtractionManifest::new([std::path::PathBuf::from(
        filename,
    )]))
}

fn manifest_entry_path(line: &str) -> (std::path::PathBuf, Vec<u8>) {
    let (path, content) = line.split_once('\t').unwrap_or((line, ""));
    (path.into(), decode_manifest_content(content).into_bytes())
}

fn decode_manifest_content(content: &str) -> String {
    content.replace("\\t", "\t").replace("\\n", "\n")
}
