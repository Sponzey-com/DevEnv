# DevEnv Provider Manifests

This directory contains reviewable provider manifests for every built-in DevEnv tool provider.

These files are not runtime archive payloads. They describe how each supported language, SDK, or CLI tool is sourced, installed, activated, and exposed by DevEnv. Runtime release artifact metadata remains in the catalog `v1/tools/<tool>/<provider>/releases.json` payloads or in the provider metadata cache.

Provider manifests still include version support metadata:

- where version lists come from;
- which cache/catalog key is used;
- which requirement shapes are supported;
- example user-facing version requirements.

Provider manifests may include generated version seed lists when they make provider behavior reviewable or provide a bootstrap/fallback for dynamic APIs. Those lists must name their generation date and source. They are not a replacement for refreshed artifact metadata, checksums, and platform-specific download links.

Recommended layout:

```text
metadata/providers/<tool>/<provider>/manifest.json
```

Rules:

- Keep one manifest per `tool` and `provider`.
- Keep these manifests aligned with `crates/devenv-tools/src/providers.rs`.
- Do not store runtime archives here.
- Add a manifest before adding a new built-in provider.
- Use catalog release payloads or provider caches for authoritative artifact lists, checksums, and download URLs.
