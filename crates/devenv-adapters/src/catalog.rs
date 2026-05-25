use std::path::{Component, Path, PathBuf};

use devenv_core::{
    CatalogEntry, CatalogFetchRequest, CatalogFetchResponse, CatalogManifest,
    CatalogPayloadDescriptor, CatalogSource, CatalogTrustFailure, CatalogTrustVerifier,
    CatalogVerificationResult, CoreError, CoreResult, MetadataCacheKey, MetadataHttpClient,
    MetadataHttpRequest, TrustRoot,
};
use reqwest::Url;

use crate::checksum::hex_sha256;

const MANIFEST_FILENAME: &str = "manifest.json";
const MANIFEST_SIGNATURE_FILENAME: &str = "manifest.sig";
const DEFAULT_FETCHED_AT: &str = "unknown";
const DEFAULT_MAX_MANIFEST_BYTES: usize = 1024 * 1024;
const DEFAULT_MAX_PAYLOAD_BYTES: usize = 4 * 1024 * 1024;
const DEFAULT_MAX_SIGNATURE_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct CatalogFetchAdapter<C> {
    root: CatalogRoot,
    http_client: C,
    fetched_at: String,
    max_payload_bytes: usize,
    max_signature_bytes: usize,
}

impl<C> CatalogFetchAdapter<C>
where
    C: MetadataHttpClient,
{
    pub fn new(root_reference: impl AsRef<str>, http_client: C) -> CoreResult<Self> {
        Ok(Self {
            root: CatalogRoot::parse(root_reference.as_ref())?,
            http_client,
            fetched_at: DEFAULT_FETCHED_AT.to_owned(),
            max_payload_bytes: DEFAULT_MAX_PAYLOAD_BYTES,
            max_signature_bytes: DEFAULT_MAX_SIGNATURE_BYTES,
        })
    }

    pub fn with_fetched_at(mut self, fetched_at: impl Into<String>) -> Self {
        self.fetched_at = fetched_at.into();
        self
    }

    pub fn with_max_payload_bytes(mut self, bytes: usize) -> Self {
        self.max_payload_bytes = bytes;
        self
    }

    pub fn with_max_signature_bytes(mut self, bytes: usize) -> Self {
        self.max_signature_bytes = bytes;
        self
    }

    pub fn root_reference(&self) -> &str {
        self.root.reference()
    }

    pub fn fetch_and_verify_manifest(
        &mut self,
        request: &CatalogFetchRequest,
        verifier: &mut dyn CatalogTrustVerifier,
        trust_root: &TrustRoot,
    ) -> CoreResult<CatalogFetchResponse> {
        let manifest = self.fetch_manifest(request)?;
        let signature = self.fetch_manifest_signature()?;
        let result = verifier.verify_manifest(manifest.bytes(), &signature, trust_root)?;

        match result {
            CatalogVerificationResult::Trusted { .. } => Ok(manifest),
            CatalogVerificationResult::Rejected { failure } => {
                Err(CoreError::catalog_trust(failure))
            }
        }
    }

    pub fn fetch_manifest_entry_payload(
        &mut self,
        manifest: &CatalogManifest,
        key: &MetadataCacheKey,
    ) -> CoreResult<CatalogFetchResponse> {
        let entry = manifest.entry_for(key).ok_or_else(|| {
            CoreError::message(format!(
                "catalog manifest `{}` does not contain an entry for {}/{}",
                manifest.catalog_id(),
                key.tool().as_str(),
                key.provider().as_str()
            ))
        })?;

        self.fetch_entry_payload(entry)
    }

    pub fn fetch_entry_payload(
        &mut self,
        entry: &CatalogEntry,
    ) -> CoreResult<CatalogFetchResponse> {
        self.fetch_payload(entry.descriptor())
    }

    fn fetch_manifest_signature(&mut self) -> CoreResult<Vec<u8>> {
        self.fetch_root_relative(MANIFEST_SIGNATURE_FILENAME, self.max_signature_bytes)
            .map(|response| response.into_bytes())
    }

    fn fetch_root_relative(
        &mut self,
        relative_path: &str,
        max_bytes: usize,
    ) -> CoreResult<CatalogFetchResponse> {
        validate_catalog_relative_path(relative_path)?;
        let source_reference = self.root.resolve(relative_path)?;
        let bytes = match &source_reference {
            CatalogResolvedReference::File(path) => {
                read_file_limited(path, max_bytes, relative_path)?
            }
            CatalogResolvedReference::Http(url) => {
                self.fetch_http_limited(url.as_str(), max_bytes)?
            }
        };

        Ok(CatalogFetchResponse::new(
            source_reference.as_display(),
            self.fetched_at.clone(),
            bytes,
        ))
    }

    fn fetch_http_limited(&mut self, url: &str, max_bytes: usize) -> CoreResult<Vec<u8>> {
        let response = self
            .http_client
            .fetch_metadata(&MetadataHttpRequest::new(url).with_max_body_bytes(max_bytes))
            .map_err(|error| {
                CoreError::catalog_network(format!("failed to fetch catalog URL `{url}`: {error}"))
            })?;

        if !(200..=299).contains(&response.status()) {
            return Err(CoreError::catalog_network(format!(
                "catalog HTTP request to `{url}` failed with status {}",
                response.status()
            )));
        }

        let bytes = response.into_body();
        ensure_max_bytes(url, bytes.len(), max_bytes)?;
        Ok(bytes)
    }
}

impl<C> CatalogSource for CatalogFetchAdapter<C>
where
    C: MetadataHttpClient,
{
    fn fetch_manifest(
        &mut self,
        request: &CatalogFetchRequest,
    ) -> CoreResult<CatalogFetchResponse> {
        if request.reference() != self.root.reference() {
            return Err(CoreError::message(format!(
                "catalog request reference `{}` does not match adapter root `{}`",
                request.reference(),
                self.root.reference()
            )));
        }

        let max_bytes = request.max_manifest_bytes().min(DEFAULT_MAX_MANIFEST_BYTES);
        self.fetch_root_relative(MANIFEST_FILENAME, max_bytes)
    }

    fn fetch_payload(
        &mut self,
        descriptor: &CatalogPayloadDescriptor,
    ) -> CoreResult<CatalogFetchResponse> {
        let response = self.fetch_root_relative(descriptor.path(), self.max_payload_bytes)?;
        verify_payload_sha256(descriptor, response.bytes())?;
        Ok(response)
    }
}

#[derive(Debug, Clone)]
enum CatalogRoot {
    File { reference: String, root: PathBuf },
    Http { reference: String, base_url: Url },
}

impl CatalogRoot {
    fn parse(reference: &str) -> CoreResult<Self> {
        if reference.trim().is_empty() {
            return Err(CoreError::message(
                "invalid catalog root: expected file, http, or https URL",
            ));
        }

        if reference.starts_with("file://") {
            let url = Url::parse(reference).map_err(|error| {
                CoreError::message(format!("invalid catalog file URL `{reference}`: {error}"))
            })?;
            let root = url.to_file_path().map_err(|_| {
                CoreError::message(format!(
                    "invalid catalog file URL `{reference}`: expected an absolute file path"
                ))
            })?;
            return Ok(Self::File {
                reference: reference.to_owned(),
                root,
            });
        }

        if reference.starts_with("http://") || reference.starts_with("https://") {
            let mut base_url = Url::parse(reference).map_err(|error| {
                CoreError::message(format!("invalid catalog URL `{reference}`: {error}"))
            })?;
            ensure_url_path_trailing_slash(&mut base_url);
            return Ok(Self::Http {
                reference: reference.to_owned(),
                base_url,
            });
        }

        Err(CoreError::message(format!(
            "invalid catalog root `{reference}`: supported schemes are file, http, and https"
        )))
    }

    fn reference(&self) -> &str {
        match self {
            Self::File { reference, .. } | Self::Http { reference, .. } => reference,
        }
    }

    fn resolve(&self, relative_path: &str) -> CoreResult<CatalogResolvedReference> {
        validate_catalog_relative_path(relative_path)?;
        match self {
            Self::File { root, .. } => {
                resolve_file_catalog_path(root, relative_path).map(CatalogResolvedReference::File)
            }
            Self::Http { base_url, .. } => {
                let url = base_url.join(relative_path).map_err(|error| {
                    CoreError::message(format!(
                        "invalid catalog relative path `{relative_path}` for `{base_url}`: {error}"
                    ))
                })?;
                Ok(CatalogResolvedReference::Http(url))
            }
        }
    }
}

#[derive(Debug, Clone)]
enum CatalogResolvedReference {
    File(PathBuf),
    Http(Url),
}

impl CatalogResolvedReference {
    fn as_display(&self) -> String {
        match self {
            Self::File(path) => format!("file://{}", path.display()),
            Self::Http(url) => url.to_string(),
        }
    }
}

fn ensure_url_path_trailing_slash(url: &mut Url) {
    if !url.path().ends_with('/') {
        let mut path = url.path().to_owned();
        path.push('/');
        url.set_path(&path);
    }
}

fn validate_catalog_relative_path(path: &str) -> CoreResult<()> {
    if path.trim().is_empty() {
        return Err(CoreError::message(
            "invalid catalog payload path: expected a non-empty relative path",
        ));
    }

    let path = Path::new(path);
    for component in path.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CoreError::message(format!(
                    "invalid catalog payload path `{}`: path traversal is not allowed",
                    path.display()
                )));
            }
        }
    }
    Ok(())
}

fn resolve_file_catalog_path(root: &Path, relative_path: &str) -> CoreResult<PathBuf> {
    let root = root.canonicalize().map_err(|error| {
        CoreError::message(format!(
            "failed to resolve catalog file root `{}`: {error}",
            root.display()
        ))
    })?;
    let candidate = root.join(relative_path);
    let canonical = candidate.canonicalize().map_err(|error| {
        CoreError::message(format!(
            "failed to resolve catalog file `{}`: {error}",
            candidate.display()
        ))
    })?;

    if !canonical.starts_with(&root) {
        return Err(CoreError::message(format!(
            "invalid catalog file `{}`: resolved path is outside catalog root `{}`",
            canonical.display(),
            root.display()
        )));
    }

    Ok(canonical)
}

fn read_file_limited(path: &Path, max_bytes: usize, context: &str) -> CoreResult<Vec<u8>> {
    let metadata = std::fs::metadata(path).map_err(|error| {
        CoreError::message(format!(
            "failed to read catalog file `{}` metadata for `{context}`: {error}",
            path.display()
        ))
    })?;
    let len = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
    ensure_max_bytes(context, len, max_bytes)?;

    let bytes = std::fs::read(path).map_err(|error| {
        CoreError::message(format!(
            "failed to read catalog file `{}` for `{context}`: {error}",
            path.display()
        ))
    })?;
    ensure_max_bytes(context, bytes.len(), max_bytes)?;
    Ok(bytes)
}

fn ensure_max_bytes(context: &str, actual: usize, max: usize) -> CoreResult<()> {
    if actual > max {
        Err(CoreError::message(format!(
            "catalog payload `{context}` exceeded max size of {max} bytes; got {actual} bytes"
        )))
    } else {
        Ok(())
    }
}

fn verify_payload_sha256(descriptor: &CatalogPayloadDescriptor, bytes: &[u8]) -> CoreResult<()> {
    let actual = format!("sha256:{}", hex_sha256(bytes));
    if descriptor.sha256() == actual {
        return Ok(());
    }

    Err(CoreError::catalog_trust(
        CatalogTrustFailure::ChecksumMismatch {
            path: descriptor.path().to_owned(),
            expected: descriptor.sha256().to_owned(),
            actual,
        },
    ))
}
