use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::thread;

use devenv_adapters::checksum::hex_sha256;
use devenv_adapters::download::{CachedArtifactDownloader, FileDownloader};
use devenv_core::{ArchiveType, Artifact, Downloader};

#[test]
fn http_artifact_download_success() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let body = b"runtime archive";
    let checksum = format!("sha256:{}", hex_sha256(body));
    let url = serve_response(200, body, None);
    let artifact = Artifact::new(url, "runtime.tar.gz", ArchiveType::TarGz, Some(checksum));
    let mut downloader = CachedArtifactDownloader::new(temp.path().join("cache/downloads"))
        .expect("downloader should build");
    let destination = temp.path().join("downloaded.tar.gz");

    let downloaded = downloader
        .download(&artifact, &destination)
        .expect("artifact should download");

    assert_eq!(downloaded.path(), destination.as_path());
    assert_eq!(
        std::fs::read(&destination).expect("download should exist"),
        body
    );
}

#[test]
fn local_file_artifact_download_still_works() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let source = temp.path().join("fixture.archive");
    std::fs::write(&source, "fixture").expect("fixture should be written");
    let artifact = Artifact::new(
        format!("file://{}", source.display()),
        "fixture.archive",
        ArchiveType::PlainFile,
        None,
    );
    let mut downloader = FileDownloader;
    let destination = temp.path().join("downloaded.archive");

    downloader
        .download(&artifact, &destination)
        .expect("local artifact should download");

    assert_eq!(
        std::fs::read_to_string(destination).expect("download should exist"),
        "fixture"
    );
}

#[test]
fn checksum_success_promotes_http_artifact_to_download_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let body = b"cached runtime";
    let checksum = format!("sha256:{}", hex_sha256(body));
    let artifact = Artifact::new(
        serve_response(200, body, None),
        "runtime.tar.gz",
        ArchiveType::TarGz,
        Some(checksum.clone()),
    );
    let mut downloader = CachedArtifactDownloader::new(temp.path().join("cache/downloads"))
        .expect("downloader should build");

    downloader
        .download(&artifact, &temp.path().join("downloaded.tar.gz"))
        .expect("artifact should download");

    let cache_path = downloader
        .cache_path_for_artifact(&artifact)
        .expect("cache path should resolve")
        .expect("checksum cache path should exist");
    assert_eq!(std::fs::read(cache_path).expect("cache should exist"), body);
}

#[test]
fn checksum_mismatch_does_not_promote_http_artifact_to_download_cache() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let artifact = Artifact::new(
        serve_response(200, b"bad runtime", None),
        "runtime.tar.gz",
        ArchiveType::TarGz,
        Some(format!("sha256:{}", "0".repeat(64))),
    );
    let mut downloader = CachedArtifactDownloader::new(temp.path().join("cache/downloads"))
        .expect("downloader should build");

    let error = downloader
        .download(&artifact, &temp.path().join("downloaded.tar.gz"))
        .expect_err("checksum mismatch should fail");

    assert!(error.to_string().contains("checksum mismatch"));
    let cache_path = downloader
        .cache_path_for_artifact(&artifact)
        .expect("cache path should resolve")
        .expect("checksum cache path should exist");
    assert!(!cache_path.exists());
}

#[test]
fn failed_partial_http_download_is_cleaned_up() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let artifact = Artifact::new(
        serve_truncated_response(b"partial"),
        "runtime.tar.gz",
        ArchiveType::TarGz,
        Some(format!("sha256:{}", "1".repeat(64))),
    );
    let cache_dir = temp.path().join("cache/downloads");
    let mut downloader =
        CachedArtifactDownloader::new(&cache_dir).expect("downloader should build");

    let error = downloader
        .download(&artifact, &temp.path().join("downloaded.tar.gz"))
        .expect_err("truncated response should fail");

    assert!(
        error
            .to_string()
            .contains("failed to read artifact HTTP response")
    );
    assert_tmp_dir_empty_or_missing(&cache_dir.join("tmp"));
}

#[test]
fn already_cached_artifact_is_reused_without_http_request() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let body = b"already cached";
    let checksum = format!("sha256:{}", hex_sha256(body));
    let artifact = Artifact::new(
        "http://127.0.0.1:9/not-called",
        "runtime.tar.gz",
        ArchiveType::TarGz,
        Some(checksum),
    );
    let mut downloader = CachedArtifactDownloader::new(temp.path().join("cache/downloads"))
        .expect("downloader should build");
    let cache_path = downloader
        .cache_path_for_artifact(&artifact)
        .expect("cache path should resolve")
        .expect("checksum cache path should exist");
    std::fs::create_dir_all(cache_path.parent().expect("cache should have parent"))
        .expect("cache dir should be created");
    std::fs::write(&cache_path, body).expect("cache should be written");

    downloader
        .download(&artifact, &temp.path().join("downloaded.tar.gz"))
        .expect("cached artifact should be reused");

    assert_eq!(
        std::fs::read(temp.path().join("downloaded.tar.gz")).expect("download should exist"),
        body
    );
}

#[test]
fn unsupported_url_scheme_is_actionable() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let artifact = Artifact::new(
        "ftp://example.test/runtime.tar.gz",
        "runtime.tar.gz",
        ArchiveType::TarGz,
        None,
    );
    let mut downloader = CachedArtifactDownloader::new(temp.path().join("cache/downloads"))
        .expect("downloader should build");

    let error = downloader
        .download(&artifact, &temp.path().join("downloaded.tar.gz"))
        .expect_err("unsupported scheme should fail");

    assert!(error.to_string().contains("unsupported artifact URL"));
}

fn serve_response(status: u16, body: &'static [u8], content_length: Option<usize>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("server should bind");
    let address = listener.local_addr().expect("server address should exist");
    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("request should arrive");
        let mut request = [0_u8; 1024];
        let _ = stream.read(&mut request);
        let reason = if status == 200 { "OK" } else { "ERROR" };
        let content_length = content_length.unwrap_or(body.len());
        write!(
            stream,
            "HTTP/1.1 {status} {reason}\r\nContent-Length: {content_length}\r\nConnection: close\r\n\r\n"
        )
        .expect("headers should write");
        stream.write_all(body).expect("body should write");
    });
    format!("http://{address}/artifact")
}

fn serve_truncated_response(body: &'static [u8]) -> String {
    serve_response(200, body, Some(body.len() + 100))
}

fn assert_tmp_dir_empty_or_missing(path: &Path) {
    if !path.exists() {
        return;
    }
    let entries = std::fs::read_dir(path)
        .expect("tmp dir should be readable")
        .collect::<Result<Vec<_>, _>>()
        .expect("tmp entries should be readable");
    assert!(entries.is_empty(), "tmp directory should be empty");
}
