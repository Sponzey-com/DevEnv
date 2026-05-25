use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use devenv_adapters::checksum::hex_sha256;
use devenv_catalog::{GenerateOptions, generate_catalog, verify_catalog};
use serde_json::Value;

const GENERATED_AT: &str = "2026-05-22T00:00:00Z";
const EXPIRES_AT: &str = "2026-05-29T00:00:00Z";
const CATALOG_VERSION: &str = "2026.05.22.1";

#[test]
fn catalog_generator_output_is_deterministic() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let source = write_source_fixture(temp.path());
    let overrides = write_override_fixture(temp.path());
    let first = temp.path().join("out-a");
    let second = temp.path().join("out-b");

    generate_catalog(&generate_options(&source, &first, &overrides))
        .expect("first catalog generation should succeed");
    generate_catalog(&generate_options(&source, &second, &overrides))
        .expect("second catalog generation should succeed");

    assert_eq!(snapshot_dir(&first), snapshot_dir(&second));
}

#[test]
fn catalog_generator_manifest_entry_sha256_matches_payload() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let source = write_source_fixture(temp.path());
    let overrides = write_override_fixture(temp.path());
    let output = temp.path().join("out");

    generate_catalog(&generate_options(&source, &output, &overrides))
        .expect("catalog generation should succeed");
    let manifest = read_json(&output.join("manifest.json"));
    let entries = manifest["entries"]
        .as_array()
        .expect("entries should be an array");

    for entry in entries {
        let path = entry["path"].as_str().expect("path should exist");
        let payload = fs::read(output.join(path)).expect("payload should be readable");
        assert_eq!(
            entry["sha256"].as_str(),
            Some(format!("sha256:{}", hex_sha256(&payload)).as_str())
        );
    }
}

#[test]
fn catalog_generator_verifier_rejects_modified_payload() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let source = write_source_fixture(temp.path());
    let overrides = write_override_fixture(temp.path());
    let output = temp.path().join("out");

    generate_catalog(&generate_options(&source, &output, &overrides))
        .expect("catalog generation should succeed");
    fs::write(output.join("tools/go/official/releases.json"), "{}\n")
        .expect("payload should be modified");

    let error = verify_catalog(&output).expect_err("modified payload should fail verification");
    assert!(error.to_string().contains("payload checksum mismatch"));
}

#[test]
fn catalog_generator_override_is_reflected() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let source = write_source_fixture(temp.path());
    let overrides = write_override_fixture(temp.path());
    let output = temp.path().join("out");

    generate_catalog(&generate_options(&source, &output, &overrides))
        .expect("catalog generation should succeed");
    let go = read_json(&output.join("tools/go/official/releases.json"));
    let release = go["releases"]
        .as_array()
        .expect("releases should be an array")
        .iter()
        .find(|release| release["version"] == "1.21.0")
        .expect("overridden release should exist");

    assert_eq!(release["stable"], false);
    assert_eq!(release["yanked"], true);
    assert_eq!(release["deprecated"], true);
    assert_eq!(
        release["reason"].as_str(),
        Some("manual yanked release for generator test")
    );
}

#[test]
fn catalog_generator_output_has_stable_ordering() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let source = write_source_fixture(temp.path());
    let overrides = write_override_fixture(temp.path());
    let output = temp.path().join("out");

    generate_catalog(&generate_options(&source, &output, &overrides))
        .expect("catalog generation should succeed");
    let go = read_json(&output.join("tools/go/official/releases.json"));
    let versions = go["releases"]
        .as_array()
        .expect("releases should be an array")
        .iter()
        .map(|release| release["version"].as_str().expect("version should exist"))
        .collect::<Vec<_>>();
    let artifact_names = go["releases"][0]["artifacts"]
        .as_array()
        .expect("artifacts should be an array")
        .iter()
        .map(|artifact| {
            artifact["filename"]
                .as_str()
                .expect("filename should exist")
        })
        .collect::<Vec<_>>();

    assert_eq!(versions, vec!["1.22.5", "1.21.0"]);
    assert_eq!(
        artifact_names,
        vec![
            "go1.22.5.darwin-arm64.tar.gz",
            "go1.22.5.linux-amd64.tar.gz"
        ]
    );
}

#[test]
fn catalog_generator_missing_checksum_is_non_installable() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let source = write_source_fixture(temp.path());
    let overrides = write_override_fixture(temp.path());
    let output = temp.path().join("out");

    generate_catalog(&generate_options(&source, &output, &overrides))
        .expect("catalog generation should succeed");
    let node = read_json(&output.join("tools/node/official/releases.json"));
    let artifact = node["releases"]
        .as_array()
        .expect("releases should be an array")
        .iter()
        .find(|release| release["version"] == "16.20.2")
        .and_then(|release| release["artifacts"].as_array())
        .and_then(|artifacts| artifacts.first())
        .expect("checksumless artifact should exist");

    assert_eq!(artifact["checksum"], Value::Null);
    assert_eq!(artifact["checksum_algorithm"], Value::Null);
    assert_eq!(artifact["installable"], false);
    assert_eq!(
        artifact["install_block_reason"].as_str(),
        Some("missing checksum")
    );
}

#[test]
fn catalog_generator_verifies_generated_catalog() {
    let temp = tempfile::tempdir().expect("tempdir should be created");
    let source = write_source_fixture(temp.path());
    let overrides = write_override_fixture(temp.path());
    let output = temp.path().join("out");

    let generated = generate_catalog(&generate_options(&source, &output, &overrides))
        .expect("catalog generation should succeed");
    let verified = verify_catalog(&output).expect("generated catalog should verify");

    assert_eq!(generated.entries, 2);
    assert_eq!(verified.entries, 2);
}

fn generate_options(source: &Path, output: &Path, overrides: &Path) -> GenerateOptions {
    GenerateOptions::new(source, output, GENERATED_AT, EXPIRES_AT, CATALOG_VERSION)
        .with_overrides_path(overrides)
}

fn write_source_fixture(parent: &Path) -> PathBuf {
    let source = parent.join("source");
    fs::create_dir_all(source.join("go/official")).expect("go source dir should be created");
    fs::create_dir_all(source.join("node/official/shasums/v20.11.1"))
        .expect("node shasums dir should be created");
    fs::write(
        source.join("go/official/releases.json"),
        r#"
[
  {
    "version": "go1.21.0",
    "stable": true,
    "files": [
      {
        "filename": "go1.21.0.linux-amd64.tar.gz",
        "os": "linux",
        "arch": "amd64",
        "kind": "archive",
        "sha256": "3333333333333333333333333333333333333333333333333333333333333333",
        "size": 33
      }
    ]
  },
  {
    "version": "go1.22.5",
    "stable": true,
    "files": [
      {
        "filename": "go1.22.5.linux-amd64.tar.gz",
        "os": "linux",
        "arch": "amd64",
        "kind": "archive",
        "sha256": "2222222222222222222222222222222222222222222222222222222222222222",
        "size": 22
      },
      {
        "filename": "go1.22.5.darwin-arm64.tar.gz",
        "os": "darwin",
        "arch": "arm64",
        "kind": "archive",
        "sha256": "1111111111111111111111111111111111111111111111111111111111111111",
        "size": 11
      }
    ]
  }
]
"#,
    )
    .expect("go source should be written");
    fs::write(
        source.join("node/official/index.json"),
        r#"
[
  {
    "version": "v16.20.2",
    "date": "2023-08-08",
    "files": ["linux-x64"],
    "lts": false
  },
  {
    "version": "v20.11.1",
    "date": "2024-02-13",
    "files": ["linux-x64", "osx-arm64-tar"],
    "lts": "Iron"
  }
]
"#,
    )
    .expect("node index should be written");
    fs::write(
        source.join("node/official/shasums/v20.11.1/SHASUMS256.txt"),
        r#"
4444444444444444444444444444444444444444444444444444444444444444  node-v20.11.1-darwin-arm64.tar.gz
5555555555555555555555555555555555555555555555555555555555555555  node-v20.11.1-linux-x64.tar.gz
"#,
    )
    .expect("node shasums should be written");
    source
}

fn write_override_fixture(parent: &Path) -> PathBuf {
    let overrides = parent.join("overrides.toml");
    fs::write(
        &overrides,
        r#"
[[release]]
tool = "go"
version = "1.21.0"
stable = false
yanked = true
deprecated = true
reason = "manual yanked release for generator test"
"#,
    )
    .expect("overrides should be written");
    overrides
}

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).expect("json file should be readable"))
        .expect("json should parse")
}

fn snapshot_dir(root: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut snapshot = BTreeMap::new();
    collect_snapshot(root, root, &mut snapshot);
    snapshot
}

fn collect_snapshot(root: &Path, current: &Path, snapshot: &mut BTreeMap<String, Vec<u8>>) {
    let mut entries = fs::read_dir(current)
        .expect("directory should be readable")
        .map(|entry| entry.expect("entry should be readable").path())
        .collect::<Vec<_>>();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            collect_snapshot(root, &path, snapshot);
        } else {
            let relative = path
                .strip_prefix(root)
                .expect("path should be under root")
                .to_string_lossy()
                .replace('\\', "/");
            snapshot.insert(relative, fs::read(path).expect("file should be readable"));
        }
    }
}
