use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use devenv_core::{SupportLevel, ToolAdapter};
use devenv_tools::{builtin_provider_registry, builtin_tool_adapter};
use serde_json::Value;

#[test]
fn provider_manifests_match_builtin_provider_registry() {
    let manifest_root = repo_root().join("metadata/providers");
    let registry = builtin_provider_registry();
    let mut expected_manifest_keys = BTreeSet::new();

    for capability in registry.providers() {
        let tool = capability.tool().as_str();
        let provider = capability.provider().as_str();
        let key = format!("{tool}/{provider}");
        let manifest_path = manifest_root
            .join(tool)
            .join(provider)
            .join("manifest.json");
        let manifest = read_manifest(&manifest_path);

        expected_manifest_keys.insert(key.clone());

        assert_eq!(
            number_field(&manifest, "schema_version"),
            1,
            "{key} manifest schema version should be v1"
        );
        assert_eq!(string_field(&manifest, "tool"), tool, "{key} tool mismatch");
        assert_eq!(
            string_field(&manifest, "provider"),
            provider,
            "{key} provider mismatch"
        );
        assert_eq!(
            string_field(&manifest, "display_name"),
            capability.display_name(),
            "{key} display name mismatch"
        );
        assert_eq!(
            string_field(&manifest, "support_level"),
            capability.support_level().as_str(),
            "{key} support level mismatch"
        );
        assert_eq!(
            string_field(&manifest, "source_kind"),
            capability.source_kind().as_str(),
            "{key} source kind mismatch"
        );
        assert_eq!(
            string_field(&manifest, "checksum_policy"),
            capability.checksum_policy().as_str(),
            "{key} checksum policy mismatch"
        );

        let expected_selectors = capability
            .selector_dimensions()
            .iter()
            .map(|dimension| dimension.as_str().to_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            string_array_field(&manifest, "selectors"),
            expected_selectors,
            "{key} selector dimensions mismatch"
        );

        let expected_platforms = capability
            .platform_support()
            .platforms()
            .iter()
            .map(|platform| platform.id())
            .collect::<Vec<_>>();
        assert_eq!(
            string_array_field(&manifest, "platforms"),
            expected_platforms,
            "{key} platform list mismatch"
        );

        let adapter = builtin_tool_adapter(capability.tool());
        assert_eq!(
            string_array_field(&manifest, "exposed_binaries"),
            adapter.exposed_binaries(),
            "{key} exposed binaries mismatch"
        );

        assert_eq!(
            nested_bool_field(&manifest, "install", "direct_install"),
            capability.support_level() == SupportLevel::Direct,
            "{key} direct install flag mismatch"
        );
        assert!(
            !nested_string_array_field(&manifest, "version", "supported_requirements").is_empty(),
            "{key} should describe supported version requirement shapes"
        );
        assert!(
            !nested_string_array_field(&manifest, "version", "requirement_examples").is_empty(),
            "{key} should include version requirement examples"
        );
        assert_eq!(
            nested_string_field(&manifest, "version", "metadata", "cache_key"),
            key,
            "{key} version metadata cache key mismatch"
        );
        assert!(
            !nested_string_field(&manifest, "version", "metadata", "mode").is_empty(),
            "{key} should describe version metadata mode"
        );
        assert_eq!(
            nested_nested_value_field(&manifest, "version", "metadata", "catalog_payload"),
            nested_value_field(&manifest, "catalog", "payload_path"),
            "{key} version metadata should point at the same catalog payload path"
        );
        assert_known_versions_are_explicit(&key, &manifest);
        if key == "java/temurin" {
            assert_java_temurin_known_versions(&manifest);
        }
    }

    assert_eq!(
        collect_manifest_keys(&manifest_root),
        expected_manifest_keys,
        "provider manifests should have no missing or extra built-in provider entries"
    );
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_manifest(path: &Path) -> Value {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|error| panic!("failed to parse {} as JSON: {error}", path.display()))
}

fn collect_manifest_keys(root: &Path) -> BTreeSet<String> {
    let mut keys = BTreeSet::new();
    for tool_entry in fs::read_dir(root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()))
    {
        let tool_entry = tool_entry.expect("tool manifest directory entry should be readable");
        if !tool_entry.path().is_dir() {
            continue;
        }
        let tool = tool_entry.file_name().to_string_lossy().into_owned();
        for provider_entry in fs::read_dir(tool_entry.path())
            .unwrap_or_else(|error| panic!("failed to read provider dir for {tool}: {error}"))
        {
            let provider_entry =
                provider_entry.expect("provider manifest directory entry should be readable");
            if !provider_entry.path().is_dir() {
                continue;
            }
            let provider = provider_entry.file_name().to_string_lossy().into_owned();
            if provider_entry.path().join("manifest.json").exists() {
                keys.insert(format!("{tool}/{provider}"));
            }
        }
    }
    keys
}

fn assert_java_temurin_known_versions(manifest: &Value) {
    let known_versions = nested_value_field(manifest, "version", "known_versions");
    assert!(
        !string_field(known_versions, "generated_at").is_empty(),
        "java/temurin known version seed should record generation date"
    );
    assert!(
        !string_field(known_versions, "source").is_empty(),
        "java/temurin known version seed should record source"
    );
    assert!(
        !string_field(known_versions, "scope").is_empty(),
        "java/temurin known version seed should record scope"
    );

    let features = number_array_field(known_versions, "feature_releases");
    assert!(
        !features.is_empty(),
        "java/temurin should list feature releases available for bootstrap"
    );
    let lts_features = number_array_field(known_versions, "lts_feature_releases");
    assert!(
        lts_features
            .iter()
            .all(|feature| features.contains(feature)),
        "java/temurin LTS feature releases should be part of feature releases"
    );

    let by_feature = known_versions
        .get("by_feature")
        .and_then(Value::as_object)
        .expect("expected object field `known_versions.by_feature`");
    let all_versions = string_array_field(known_versions, "versions");
    assert!(
        !all_versions.is_empty(),
        "java/temurin should include a flat installable version seed list"
    );
    for feature in features {
        let versions = by_feature
            .get(&feature.to_string())
            .and_then(Value::as_array)
            .unwrap_or_else(|| {
                panic!("java/temurin known versions should include feature `{feature}`")
            });
        assert!(
            !versions.is_empty(),
            "java/temurin feature `{feature}` should have at least one known install version"
        );
        for version in versions {
            let version = version
                .as_str()
                .expect("java/temurin known version entries should be strings");
            assert!(
                version == feature.to_string() || version.starts_with(&format!("{feature}.")),
                "java/temurin known version `{version}` should belong to feature `{feature}`"
            );
        }
    }
}

fn assert_known_versions_are_explicit(key: &str, manifest: &Value) {
    let known_versions = nested_value_field(manifest, "version", "known_versions");
    assert!(
        !string_field(known_versions, "generated_at").is_empty(),
        "{key} known version metadata should record generation date"
    );
    assert!(
        !string_field(known_versions, "source").is_empty(),
        "{key} known version metadata should record source"
    );
    assert!(
        !string_field(known_versions, "scope").is_empty(),
        "{key} known version metadata should record scope"
    );

    let versions = string_array_field(known_versions, "versions");
    assert!(
        !versions.is_empty(),
        "{key} should include known version seeds because version selection is part of the provider contract"
    );
    let support_level = string_field(manifest, "support_level");
    if support_level != "direct" {
        assert!(
            known_versions
                .get("status")
                .and_then(Value::as_str)
                .is_some(),
            "{key} non-remote provider should explain why known versions are not enumerated"
        );
    }
}

fn number_field(value: &Value, field: &str) -> i64 {
    value
        .get(field)
        .and_then(Value::as_i64)
        .unwrap_or_else(|| panic!("expected numeric field `{field}`"))
}

fn string_field<'a>(value: &'a Value, field: &str) -> &'a str {
    value
        .get(field)
        .and_then(Value::as_str)
        .unwrap_or_else(|| panic!("expected string field `{field}`"))
}

fn string_array_field(value: &Value, field: &str) -> Vec<String> {
    value
        .get(field)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected array field `{field}`"))
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .unwrap_or_else(|| panic!("expected `{field}` entries to be strings"))
                .to_owned()
        })
        .collect()
}

fn number_array_field(value: &Value, field: &str) -> Vec<u64> {
    value
        .get(field)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected array field `{field}`"))
        .iter()
        .map(|entry| {
            entry
                .as_u64()
                .unwrap_or_else(|| panic!("expected `{field}` entries to be numbers"))
        })
        .collect()
}

fn nested_bool_field(value: &Value, object_field: &str, field: &str) -> bool {
    value
        .get(object_field)
        .and_then(Value::as_object)
        .and_then(|object| object.get(field))
        .and_then(Value::as_bool)
        .unwrap_or_else(|| panic!("expected boolean field `{object_field}.{field}`"))
}

fn nested_string_array_field(value: &Value, object_field: &str, field: &str) -> Vec<String> {
    value
        .get(object_field)
        .and_then(Value::as_object)
        .and_then(|object| object.get(field))
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("expected array field `{object_field}.{field}`"))
        .iter()
        .map(|entry| {
            entry
                .as_str()
                .unwrap_or_else(|| {
                    panic!("expected `{object_field}.{field}` entries to be strings")
                })
                .to_owned()
        })
        .collect()
}

fn nested_string_field<'a>(
    value: &'a Value,
    object_field: &str,
    nested_object_field: &str,
    field: &str,
) -> &'a str {
    value
        .get(object_field)
        .and_then(Value::as_object)
        .and_then(|object| object.get(nested_object_field))
        .and_then(Value::as_object)
        .and_then(|object| object.get(field))
        .and_then(Value::as_str)
        .unwrap_or_else(|| {
            panic!("expected string field `{object_field}.{nested_object_field}.{field}`")
        })
}

fn nested_value_field<'a>(value: &'a Value, object_field: &str, field: &str) -> &'a Value {
    value
        .get(object_field)
        .and_then(Value::as_object)
        .and_then(|object| object.get(field))
        .unwrap_or_else(|| panic!("expected field `{object_field}.{field}`"))
}

fn nested_nested_value_field<'a>(
    value: &'a Value,
    object_field: &str,
    nested_object_field: &str,
    field: &str,
) -> &'a Value {
    value
        .get(object_field)
        .and_then(Value::as_object)
        .and_then(|object| object.get(nested_object_field))
        .and_then(Value::as_object)
        .and_then(|object| object.get(field))
        .unwrap_or_else(|| panic!("expected field `{object_field}.{nested_object_field}.{field}`"))
}
