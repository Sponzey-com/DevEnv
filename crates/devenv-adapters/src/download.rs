use std::path::{Path, PathBuf};

use devenv_core::{Artifact, CoreError, CoreResult, DownloadedArtifact, Downloader};

#[derive(Debug, Clone, Default)]
pub struct FileDownloader;

impl Downloader for FileDownloader {
    fn download(
        &mut self,
        artifact: &Artifact,
        destination: &Path,
    ) -> CoreResult<DownloadedArtifact> {
        let source = local_artifact_path(artifact.url())?;
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CoreError::message(format!(
                    "failed to create download directory `{}`: {error}",
                    parent.display()
                ))
            })?;
        }
        std::fs::copy(&source, destination).map_err(|error| {
            CoreError::message(format!(
                "failed to copy artifact `{}` to `{}`: {error}",
                source.display(),
                destination.display()
            ))
        })?;
        let size = std::fs::metadata(destination)
            .map_err(|error| {
                CoreError::message(format!(
                    "failed to read downloaded artifact `{}` metadata: {error}",
                    destination.display()
                ))
            })?
            .len();

        Ok(DownloadedArtifact::new(destination, size))
    }
}

fn local_artifact_path(url: &str) -> CoreResult<PathBuf> {
    if let Some(path) = url.strip_prefix("file://") {
        return Ok(PathBuf::from(path));
    }

    let path = PathBuf::from(url);
    if path.is_absolute() || path.exists() {
        return Ok(path);
    }

    Err(CoreError::message(format!(
        "unsupported artifact URL `{url}`: default downloader currently supports file:// URLs or local paths"
    )))
}
