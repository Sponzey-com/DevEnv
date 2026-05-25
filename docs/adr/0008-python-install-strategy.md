# ADR 0008: Python Install Strategy

## Status

Accepted

## Context

Python is a first-wave DevEnv target, but Python runtime installation is not shaped like Go, Node.js, Flutter, Terraform, or OpenTofu.

There is no single official CPython standalone binary archive matrix that covers macOS, Linux, and Windows with a stable release index and checksum policy suitable for DevEnv's Direct provider contract. Python installations also interact with virtual environments, `pip`, `ensurepip`, `venv`, shared libraries, OpenSSL, sqlite, readline, tkinter, platform package managers, and distribution-specific patches.

DevEnv already supports Python selection, discovery, activation, shims, local registration, and fixture-backed install tests. The unresolved product decision is the live Direct install source.

## Options

### CPython source build

Build CPython from official source tarballs.

Pros:

- Uses upstream CPython sources.
- Can support exact patch versions.
- Avoids depending on a third-party binary distributor.

Cons:

- Requires compilers and platform libraries.
- Turns DevEnv into a build orchestrator.
- Makes default install behavior slower and more failure-prone.
- Requires substantial host diagnostics before it is acceptable.

### python-build compatible definitions

Use pyenv/python-build style definitions or a compatible metadata format.

Pros:

- Mirrors a proven ecosystem model.
- Covers many CPython and PyPy versions.
- Gives a path to source-build support without inventing every recipe.

Cons:

- Still requires compilers and host dependencies.
- Requires a compatibility and update policy for definitions.
- Blurs the line between DevEnv's runtime selection role and build backend role.

### uv managed Python

Delegate Python installation to uv's managed Python support.

Pros:

- Strong current ecosystem momentum.
- Fast implementation path if DevEnv treats uv as a delegated provider.
- Avoids owning Python build recipes.

Cons:

- Makes DevEnv's Python install semantics depend on uv.
- uv also manages projects/packages, which DevEnv intentionally does not own yet.
- Requires a clear boundary so DevEnv does not become a package/environment manager implicitly.

### Platform package delegation

Delegate to Homebrew, apt, dnf, winget, or other platform package managers.

Pros:

- Uses already trusted platform channels.
- Avoids source builds in DevEnv.

Cons:

- Version availability and naming vary by platform.
- Mutates system or user package manager state.
- Hard to make reproducible across machines.

### LocalOnly until a later phase

Keep live Python Direct install deferred while supporting `devenv add python <path>`, discovery, activation, shims, `.python-version`, and fixture-backed install tests.

Pros:

- Honest about current provider capability.
- Preserves offline tests and Clean Architecture boundaries.
- Allows users to combine DevEnv with pyenv, uv, conda, pixi, Homebrew, or system Python.
- Keeps package/environment management out of scope.

Cons:

- Users cannot yet rely on DevEnv alone to fetch Python from the network.
- Python support is less complete than Go or Node.js for direct installs.

## Decision

DevEnv will defer live Python Direct install for this phase.

The Python provider remains fixture-backed for install pipeline tests and controlled internal metadata experiments. User-facing live installation should be described as deferred until a later ADR chooses one of:

- uv as a delegated provider;
- python-build compatible source-build definitions;
- a trusted standalone binary source with checksums and platform coverage.

DevEnv will continue to support:

- `devenv add python <path>`;
- `DEVENV_PYTHON_CANDIDATE_PATHS`;
- `.python-version`;
- `devenv local/global/shell/use python@<version>`;
- `python`, `python3`, and `pip` shims;
- fixture-backed `DEVENV_PYTHON_RELEASE_METADATA` for offline tests and controlled experiments.

DevEnv will not, in this phase:

- compile CPython by default;
- call pyenv/python-build;
- call uv to install Python;
- call platform package managers;
- interpret virtualenv, conda, pixi, `pyproject.toml`, or lockfiles as runtime installation instructions.

## Consequences

- CLI provider status must make the Python live Direct provider status explicit.
- `metadata status python` should point to this ADR and the fixture/local registration path.
- The product can still test the generic install pipeline with Python fixture metadata without implying that live CPython downloads are production-ready.
- Future Python install work must start with a provider-specific ADR update rather than growing ad hoc logic in CLI handlers.

## Future Work

- Evaluate uv as a Delegated provider, with explicit user opt-in.
- Evaluate python-build compatible definitions behind a source-build backend.
- Document how DevEnv should coexist with pyenv, uv, conda, pixi, and system Python.
- Add diagnostics for Python runtimes discovered from external managers without taking over those managers.
