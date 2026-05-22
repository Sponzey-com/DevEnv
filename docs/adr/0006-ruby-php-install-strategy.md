# ADR 0006: Ruby And PHP Install Strategy

## Status

Accepted

## Context

Ruby and PHP are first-wave DevEnv targets, but their installation model is materially different from Java, Go, Node.js, Python, Flutter, Terraform, and OpenTofu.

Ruby installations often depend on compiler toolchains and host libraries such as OpenSSL, readline, libyaml, zlib, and libffi. PHP installations add extension choices, SAPI choices, `php.ini` layout, PECL expectations, and platform package dependencies.

Replicating `ruby-build`, `php-build`, `phpenv`, or system package manager behavior inside the first DevEnv implementation would expand the tool from version selection into build orchestration too early.

## Decision

For this phase, DevEnv supports Ruby and PHP as local runtimes only.

- DevEnv can register existing Ruby runtimes with `devenv add ruby <path>`.
- DevEnv can register existing PHP runtimes with `devenv add php <path>`.
- DevEnv can discover candidate roots from `DEVENV_RUBY_CANDIDATE_PATHS` and `DEVENV_PHP_CANDIDATE_PATHS`.
- DevEnv can select Ruby through `devenv.toml`, `.tool-versions`, `.ruby-version`, shell scope, or CLI override.
- DevEnv can select PHP through `devenv.toml`, `.tool-versions`, shell scope, or CLI override.
- DevEnv does not run system package managers, compilers, `ruby-build`, `php-build`, PECL, or extension installers in default workflows.
- DevEnv remote install for Ruby and PHP is deferred until a dedicated design covers build dependencies, binary distribution trust, checksums, and platform support.

## Consequences

- Ruby and PHP local switching works through the same adapter, registry, activation, and shim contracts as other tools.
- The default test suite remains offline and does not require compilers or host package managers.
- Users can use DevEnv alongside existing managers such as rbenv, chruby, ruby-build, phpenv, Homebrew, asdf, and mise by registering their installed runtimes.
- Remote install commands for Ruby and PHP remain unsupported in this phase.

## Future Options

Potential future install strategies:

- Binary-only installs from trusted release sources where checksums and platform coverage are strong.
- Delegated install providers such as ruby-build/php-build with explicit opt-in and clearly reported host dependencies.
- Source-build workflows isolated behind a separate build backend, never invoked by default tests.
- Package-manager integration such as Homebrew formulas, if distribution ownership and version mapping are explicit.

Any future strategy must preserve DevEnv's core boundary: runtime selection and activation stay separate from system dependency mutation.
