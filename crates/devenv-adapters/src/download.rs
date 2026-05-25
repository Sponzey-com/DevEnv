use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use devenv_core::{Artifact, CoreError, CoreResult, DownloadedArtifact, Downloader};
use reqwest::Url;
use reqwest::blocking::Client;
use reqwest::redirect::Policy;

use crate::checksum::hex_sha256;
use crate::store::DevEnvHome;

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

#[derive(Debug, Clone)]
pub struct CachedArtifactDownloader {
    download_cache_dir: PathBuf,
    client: Client,
    local: FileDownloader,
}

impl CachedArtifactDownloader {
    pub fn new(download_cache_dir: impl Into<PathBuf>) -> CoreResult<Self> {
        let client = Client::builder()
            .redirect(Policy::limited(10))
            .build()
            .map_err(|error| {
                CoreError::message(format!("failed to build artifact HTTP client: {error}"))
            })?;
        Ok(Self {
            download_cache_dir: download_cache_dir.into(),
            client,
            local: FileDownloader,
        })
    }

    pub fn at_home(home: &DevEnvHome) -> CoreResult<Self> {
        Self::new(home.download_cache_dir())
    }

    pub fn download_cache_dir(&self) -> &Path {
        &self.download_cache_dir
    }

    pub fn cache_path_for_artifact(&self, artifact: &Artifact) -> CoreResult<Option<PathBuf>> {
        let Some(checksum) = artifact.checksum() else {
            return Ok(None);
        };
        let sha256 = parse_sha256_checksum(checksum)?;
        Ok(Some(self.sha256_cache_path(sha256)))
    }

    fn sha256_cache_path(&self, sha256: &str) -> PathBuf {
        self.download_cache_dir.join("sha256").join(sha256)
    }

    fn tmp_download_path(&self) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default();
        self.download_cache_dir
            .join("tmp")
            .join(format!("artifact-{}-{nonce}.part", std::process::id()))
    }

    fn download_http_with_cache(
        &mut self,
        artifact: &Artifact,
        destination: &Path,
    ) -> CoreResult<DownloadedArtifact> {
        if let Some(cache_path) = self.cache_path_for_artifact(artifact)? {
            let checksum = artifact
                .checksum()
                .expect("cache path exists only when checksum exists");
            if cache_path.is_file() {
                match verify_file_checksum(&cache_path, checksum) {
                    Ok(()) => return copy_cached_artifact(&cache_path, destination),
                    Err(_) => {
                        std::fs::remove_file(&cache_path).map_err(|error| {
                            CoreError::message(format!(
                                "failed to remove corrupt download cache `{}`: {error}",
                                cache_path.display()
                            ))
                        })?;
                    }
                }
            }

            let tmp_path = self.tmp_download_path();
            let result = (|| {
                self.download_http_to_path(artifact.url(), &tmp_path)?;
                verify_file_checksum(&tmp_path, checksum)?;
                if let Some(parent) = cache_path.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| {
                        CoreError::message(format!(
                            "failed to create download cache directory `{}`: {error}",
                            parent.display()
                        ))
                    })?;
                }
                match std::fs::rename(&tmp_path, &cache_path) {
                    Ok(()) => {}
                    Err(error) if cache_path.is_file() => {
                        std::fs::remove_file(&tmp_path).map_err(|cleanup_error| {
                            CoreError::message(format!(
                                "failed to cleanup duplicate temporary download `{}` after rename error `{error}`: {cleanup_error}",
                                tmp_path.display()
                            ))
                        })?;
                    }
                    Err(error) => {
                        return Err(CoreError::message(format!(
                            "failed to promote download cache `{}` to `{}`: {error}",
                            tmp_path.display(),
                            cache_path.display()
                        )));
                    }
                }
                copy_cached_artifact(&cache_path, destination)
            })();

            if result.is_err() {
                let _ = std::fs::remove_file(&tmp_path);
            }

            return result;
        }

        let tmp_path = self.tmp_download_path();
        let result = (|| {
            self.download_http_to_path(artifact.url(), &tmp_path)?;
            copy_cached_artifact(&tmp_path, destination)
        })();
        let _ = std::fs::remove_file(&tmp_path);
        result
    }

    fn download_http_to_path(&mut self, url: &str, destination: &Path) -> CoreResult<()> {
        let parsed = validate_http_url(url)?;
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CoreError::message(format!(
                    "failed to create download directory `{}`: {error}",
                    parent.display()
                ))
            })?;
        }

        let mut response = self
            .client
            .get(parsed)
            .timeout(Duration::from_secs(60))
            .send()
            .map_err(|error| {
                if error.is_timeout() {
                    CoreError::message(format!("artifact HTTP request timed out for `{url}`"))
                } else {
                    CoreError::message(format!("artifact HTTP request failed for `{url}`: {error}"))
                }
            })?;
        let status = response.status();
        if !status.is_success() {
            return Err(CoreError::message(format!(
                "artifact HTTP request to `{url}` failed with status {}",
                status.as_u16()
            )));
        }

        let mut file = std::fs::File::create(destination).map_err(|error| {
            CoreError::message(format!(
                "failed to create download file `{}`: {error}",
                destination.display()
            ))
        })?;
        let mut buffer = [0_u8; 16 * 1024];
        loop {
            let read = response.read(&mut buffer).map_err(|error| {
                CoreError::message(format!(
                    "failed to read artifact HTTP response `{url}`: {error}"
                ))
            })?;
            if read == 0 {
                break;
            }
            file.write_all(&buffer[..read]).map_err(|error| {
                CoreError::message(format!(
                    "failed to write download file `{}`: {error}",
                    destination.display()
                ))
            })?;
        }
        file.sync_all().map_err(|error| {
            CoreError::message(format!(
                "failed to sync download file `{}`: {error}",
                destination.display()
            ))
        })?;

        Ok(())
    }
}

impl Downloader for CachedArtifactDownloader {
    fn download(
        &mut self,
        artifact: &Artifact,
        destination: &Path,
    ) -> CoreResult<DownloadedArtifact> {
        if is_http_url(artifact.url()) {
            return self.download_http_with_cache(artifact, destination);
        }

        self.local.download(artifact, destination)
    }
}

fn local_artifact_path(url: &str) -> CoreResult<PathBuf> {
    if is_unsupported_url(url) {
        return Err(CoreError::message(format!(
            "unsupported artifact URL `{url}`: supported schemes are http, https, file, or local paths"
        )));
    }

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

fn copy_cached_artifact(source: &Path, destination: &Path) -> CoreResult<DownloadedArtifact> {
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            CoreError::message(format!(
                "failed to create download directory `{}`: {error}",
                parent.display()
            ))
        })?;
    }
    std::fs::copy(source, destination).map_err(|error| {
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

fn verify_file_checksum(path: &Path, expected_checksum: &str) -> CoreResult<()> {
    let expected = parse_sha256_checksum(expected_checksum)?;
    let bytes = std::fs::read(path).map_err(|error| {
        CoreError::message(format!(
            "failed to read artifact `{}` for checksum verification: {error}",
            path.display()
        ))
    })?;
    let actual = hex_sha256(&bytes);
    if actual == expected {
        Ok(())
    } else {
        Err(CoreError::message(format!(
            "checksum mismatch for `{}`: expected {expected}, got {actual}",
            path.display()
        )))
    }
}

fn parse_sha256_checksum(checksum: &str) -> CoreResult<&str> {
    let value = checksum.strip_prefix("sha256:").unwrap_or(checksum);
    if value.len() == 64 && value.chars().all(|character| character.is_ascii_hexdigit()) {
        Ok(value)
    } else {
        Err(CoreError::message(format!(
            "unsupported checksum `{checksum}`: expected sha256:<hex> or raw sha256 hex"
        )))
    }
}

fn validate_http_url(url: &str) -> CoreResult<Url> {
    let parsed = Url::parse(url)
        .map_err(|error| CoreError::message(format!("invalid artifact URL `{url}`: {error}")))?;
    match parsed.scheme() {
        "http" | "https" => Ok(parsed),
        scheme => Err(CoreError::message(format!(
            "unsupported artifact URL `{url}`: unsupported scheme `{scheme}`"
        ))),
    }
}

fn is_http_url(url: &str) -> bool {
    url.starts_with("http://") || url.starts_with("https://")
}

fn is_unsupported_url(url: &str) -> bool {
    let Some(colon_index) = url.find(':') else {
        return false;
    };
    let scheme = &url[..colon_index];
    !scheme.is_empty()
        && scheme.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
        })
        && scheme != "file"
}
