# ADR 0012: Catalog Trust Policy

Status: accepted for Phase 003 implementation

## Context

The GitHub metadata catalog introduced by ADR 0011 controls which upstream runtime artifact URLs and checksums DevEnv will trust during Direct install resolution. A compromised catalog could redirect users to malicious artifacts or hide bad releases. Plain checksums in the same repository are not enough because an attacker who changes metadata can also change adjacent checksum files.

DevEnv needs a trust policy before the catalog is wired into `metadata update` or `install`.

## Decision

Catalog manifests must be verified with a pinned trust root. Payload checksums then verify individual metadata files against the trusted manifest.

The default trust root is built into the DevEnv binary. Organization mirrors may use a separate trust root only when user/global configuration or explicit environment variables opt into it.

Trust verification is required before a catalog payload can be written to DevEnv's metadata cache.

## Trust Model

The trust chain is:

```text
built-in trust root
  -> manifest signature
    -> manifest entries
      -> payload sha256
        -> normalized tool metadata
```

The manifest signature establishes authenticity. Payload checksums establish integrity and bind payload files to the signed manifest.

## Required Verification

For every catalog update:

- verify the manifest signature against a known trust root;
- verify the manifest schema version;
- verify `catalog_id`;
- verify `expires_at`;
- verify `min_devenv_version`;
- verify each requested payload sha256;
- verify payload tool/provider matches the manifest entry;
- reject manifest entries with unsafe relative paths;
- reject payloads that exceed the configured size limit.

Manifest checksum files such as `SHA256SUMS` are useful for transfer diagnostics, but they do not replace signature verification.

## Failure Classes

| Class | Examples | Cache write | Fallback |
| --- | --- | --- | --- |
| Trust failure | signature mismatch, unknown key, revoked key, manifest checksum mismatch, payload checksum mismatch | No | No silent fallback |
| Compatibility failure | unsupported schema, `min_devenv_version` too high | No | No silent fallback |
| Freshness failure | expired catalog | No fresh cache write | May use stale cache or official provider only when policy allows |
| Network failure | timeout, DNS failure, 404, GitHub 5xx | No new cache write | In `auto`, may use official provider or stale cache |

When source mode is `catalog`, all catalog failures are returned directly. When source mode is `auto`, only network/freshness failures may continue to other sources. Trust failures must be visible.

## Key Rotation

Key rotation requires:

- a new ADR or explicit ADR update;
- release notes;
- overlap period where old and new trust roots are accepted;
- tests proving old-only, new-only, and dual-signature cases;
- a planned removal date for the old trust root.

If a key is suspected to be compromised, DevEnv should ship a patch release that revokes the key and disables affected catalog versions.

## Mirror Trust Policy

Mirror operators have two supported choices:

1. Mirror DevEnv's signed catalog without modifying it. This uses the built-in trust root.
2. Publish an organization-specific catalog signed by an organization trust root. This requires explicit user/global configuration or environment variables.

Candidate environment variables:

```text
DEVENV_CATALOG_TRUST_POLICY
DEVENV_CATALOG_TRUST_ROOT
```

Project config must not silently change catalog trust roots.

## Rollback

Rollback uses immutable catalog releases. If a catalog release is bad but the key is still trusted:

- publish a newer catalog version that yanks or corrects the bad entries;
- document the affected catalog version;
- keep older immutable releases available for investigation;
- never rewrite a release asset that has already been trusted by clients.

If the trust root is compromised, rollback is not enough. A DevEnv binary update must revoke the root.

## Consequences

- Catalog support cannot become the stable default until signature verification is implemented.
- Trust failures are intentionally louder than network failures.
- Enterprise mirrors remain possible without giving project repositories authority over trust roots.
- Future transparency log or TUF integration is possible, but Phase 003 does not require a full TUF implementation.
