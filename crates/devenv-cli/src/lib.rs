use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use devenv_adapters::archive::ManifestArchiveExtractor;
use devenv_adapters::catalog::CatalogFetchAdapter;
use devenv_adapters::checksum::{Sha256ChecksumVerifier, hex_sha256};
use devenv_adapters::download::CachedArtifactDownloader;
use devenv_adapters::fs::{
    NativeConfigRepository, discover_project_config_from, read_devenv_toml_config,
};
use devenv_adapters::install::{FileInstallTransactionManager, SystemClock};
use devenv_adapters::metadata_cache::FileMetadataCache;
use devenv_adapters::metadata_http::ReqwestMetadataHttpClient;
use devenv_adapters::process::ProcessCommandRunner;
use devenv_adapters::shell::{ShellActivationRenderer, ShellSyntax};
use devenv_adapters::shim::FileShimWriter;
use devenv_adapters::store::{DevEnvHome, FileInstallStore, FileRuntimeRegistry};
use devenv_core::{
    ActivationPlan, ActivationRenderer, Architecture, CATALOG_MANIFEST_SCHEMA_VERSION,
    CatalogEntry, CatalogFetchRequest, CatalogManifest, CatalogPayloadDescriptor,
    CatalogPayloadKind, CatalogTrustFailure, CatalogTrustVerifier, CatalogVerificationResult,
    Clock, ConfigRepository, ConfigScope, CoreError, ExecCommand, InMemoryLockManager,
    InstallRuntimePorts, InstallRuntimeRequest, MetadataCache, MetadataCacheEntry,
    MetadataCacheKey, MetadataCacheStatus, MetadataFetchOutcome, MetadataHttpClient,
    MetadataHttpRequest, MetadataPayloadFetchRequest, MetadataPayloadKind, OperatingSystem,
    Platform, ProjectConfig, ProviderCapability, ProviderId, ProviderSelectorDimension,
    RegisteredRuntime, SelectionCandidate, SelectionSource, SupportLevel, ToolAdapter, ToolName,
    ToolSpec, TrustRoot, Version, VersionMatcher, VersionRequirement, VersionSource,
    activation_plan_for_selected_runtime, add_external_runtime, collect_shim_specs,
    dispatch_shim_command, fetch_metadata_payload, install_runtime, list_remote_versions,
    plan_install_runtime, rehash_shims, remove_external_runtime, resolve_tool_selection,
    tool_for_shim_binary, uninstall_runtime,
};
use devenv_tools::{
    FLUTTER_OFFICIAL_BASE_URL, FLUTTER_OFFICIAL_LINUX_RELEASES_URL,
    FLUTTER_OFFICIAL_MACOS_RELEASES_URL, FLUTTER_OFFICIAL_WINDOWS_RELEASES_URL,
    FlutterArtifactResolver, FlutterInstalledRuntimeValidator, FlutterOfficialReleaseMetadata,
    FlutterReleaseMetadata, FlutterReleaseVersionSource, FlutterRuntime, FlutterRuntimeDiscovery,
    FlutterRuntimeSource, FlutterToolAdapter, FlutterVersionMatcher, GO_OFFICIAL_METADATA_URL,
    GoArtifactResolver, GoCatalogReleaseMetadata, GoInstalledRuntimeValidator,
    GoOfficialReleaseMetadata, GoReleaseMetadata, GoReleaseVersionSource, GoRuntime,
    GoRuntimeDiscovery, GoRuntimeSource, GoToolAdapter, GoVersionMatcher, IacArtifactResolver,
    IacCatalogReleaseMetadata, IacOfficialReleaseMetadata, IacReleaseMetadata,
    IacReleaseVersionSource, IacRuntime, IacRuntimeDiscovery, IacRuntimeSource, IacTool,
    IacVersionMatcher, JavaArtifactResolver, JavaDistribution, JavaInstalledRuntimeValidator,
    JavaReleaseMetadata, JavaReleaseVersionSource, JavaRuntime, JavaRuntimeDiscovery,
    JavaRuntimeSource, JavaTemurinReleaseMetadata, JavaToolAdapter, JavaVersionMatcher,
    NODE_OFFICIAL_DIST_BASE_URL, NODE_OFFICIAL_INDEX_URL, NodeArtifactResolver,
    NodeCatalogReleaseMetadata, NodeInstalledRuntimeValidator, NodeOfficialReleaseMetadata,
    NodeReleaseMetadata, NodeReleaseVersionSource, NodeRuntime, NodeRuntimeDiscovery,
    NodeRuntimeSource, NodeToolAdapter, NodeVersionMatcher, OPENTOFU_OFFICIAL_BASE_URL,
    OPENTOFU_OFFICIAL_RELEASES_URL, OpenTofuInstalledRuntimeValidator, OpenTofuToolAdapter,
    PhpRuntime, PhpRuntimeDiscovery, PhpRuntimeSource, PhpToolAdapter, PhpVersionMatcher,
    PythonArtifactResolver, PythonInstalledRuntimeValidator, PythonReleaseMetadata,
    PythonReleaseVersionSource, PythonRuntime, PythonRuntimeDiscovery, PythonRuntimeSource,
    PythonToolAdapter, PythonVersionMatcher, RubyRuntime, RubyRuntimeDiscovery, RubyRuntimeSource,
    RubyToolAdapter, RubyVersionMatcher, RustRuntime, RustRuntimeDiscovery, RustRuntimeSource,
    RustToolAdapter, RustVersionMatcher, TERRAFORM_OFFICIAL_BASE_URL, TERRAFORM_OFFICIAL_INDEX_URL,
    TerraformInstalledRuntimeValidator, TerraformToolAdapter, builtin_provider_registry,
    builtin_tool_adapter, match_flutter_runtime, match_go_runtime, match_iac_runtime,
    match_java_runtime, match_node_runtime, match_php_runtime, match_python_runtime,
    match_ruby_runtime, match_rust_runtime, node_official_required_shasums_versions,
    normalize_flutter_version, normalize_go_version, normalize_iac_version, normalize_node_version,
    normalize_php_version, normalize_python_version, normalize_ruby_version,
    normalize_rust_version, parse_iac_sha256s, parse_node_shasums256, validate_flutter_sdk_home,
    validate_go_sdk_home, validate_iac_tool_home, validate_jdk_home, validate_node_home,
    validate_php_home, validate_python_home, validate_ruby_home, validate_rust_toolchain_home,
};

pub const COMMAND_NAME: &str = "devenv";
const INSTALL_USAGE: &str = "usage: devenv install <tool> <version> [provider-or-channel] [--dry-run] [--distribution temurin] [--channel stable]";
const UNINSTALL_USAGE: &str = "usage: devenv uninstall <tool> <version>";
const GLOBAL_CONFIG_ENV: &str = "DEVENV_GLOBAL_CONFIG";
const SHELL_ENV_PREFIX: &str = "DEVENV_TOOL_";
const JAVA_CANDIDATE_PATHS_ENV: &str = "DEVENV_JAVA_CANDIDATE_PATHS";
const GO_CANDIDATE_PATHS_ENV: &str = "DEVENV_GO_CANDIDATE_PATHS";
const FLUTTER_CANDIDATE_PATHS_ENV: &str = "DEVENV_FLUTTER_CANDIDATE_PATHS";
const TERRAFORM_CANDIDATE_PATHS_ENV: &str = "DEVENV_TERRAFORM_CANDIDATE_PATHS";
const OPENTOFU_CANDIDATE_PATHS_ENV: &str = "DEVENV_OPENTOFU_CANDIDATE_PATHS";
const NODE_CANDIDATE_PATHS_ENV: &str = "DEVENV_NODE_CANDIDATE_PATHS";
const PYTHON_CANDIDATE_PATHS_ENV: &str = "DEVENV_PYTHON_CANDIDATE_PATHS";
const RUBY_CANDIDATE_PATHS_ENV: &str = "DEVENV_RUBY_CANDIDATE_PATHS";
const PHP_CANDIDATE_PATHS_ENV: &str = "DEVENV_PHP_CANDIDATE_PATHS";
const RUST_CANDIDATE_PATHS_ENV: &str = "DEVENV_RUST_CANDIDATE_PATHS";
const RUSTUP_HOME_ENV: &str = "RUSTUP_HOME";
const GO_RELEASE_METADATA_ENV: &str = "DEVENV_GO_RELEASE_METADATA";
const GO_OFFICIAL_RELEASE_METADATA_ENV: &str = "DEVENV_GO_OFFICIAL_RELEASE_METADATA";
const FLUTTER_RELEASE_METADATA_ENV: &str = "DEVENV_FLUTTER_RELEASE_METADATA";
const FLUTTER_OFFICIAL_RELEASES_DIR_ENV: &str = "DEVENV_FLUTTER_OFFICIAL_RELEASES_DIR";
const FLUTTER_OFFICIAL_BASE_URL_ENV: &str = "DEVENV_FLUTTER_OFFICIAL_BASE_URL";
const TERRAFORM_RELEASE_METADATA_ENV: &str = "DEVENV_TERRAFORM_RELEASE_METADATA";
const TERRAFORM_OFFICIAL_RELEASE_INDEX_ENV: &str = "DEVENV_TERRAFORM_OFFICIAL_RELEASE_INDEX";
const TERRAFORM_OFFICIAL_SHA256SUMS_DIR_ENV: &str = "DEVENV_TERRAFORM_OFFICIAL_SHA256SUMS_DIR";
const TERRAFORM_OFFICIAL_BASE_URL_ENV: &str = "DEVENV_TERRAFORM_OFFICIAL_BASE_URL";
const OPENTOFU_RELEASE_METADATA_ENV: &str = "DEVENV_OPENTOFU_RELEASE_METADATA";
const OPENTOFU_OFFICIAL_RELEASES_ENV: &str = "DEVENV_OPENTOFU_OFFICIAL_RELEASES";
const OPENTOFU_OFFICIAL_SHA256SUMS_DIR_ENV: &str = "DEVENV_OPENTOFU_OFFICIAL_SHA256SUMS_DIR";
const OPENTOFU_OFFICIAL_BASE_URL_ENV: &str = "DEVENV_OPENTOFU_OFFICIAL_BASE_URL";
const JAVA_RELEASE_METADATA_ENV: &str = "DEVENV_JAVA_RELEASE_METADATA";
const JAVA_TEMURIN_RELEASE_METADATA_ENV: &str = "DEVENV_JAVA_TEMURIN_RELEASE_METADATA";
const JAVA_TEMURIN_API_BASE_URL_ENV: &str = "DEVENV_JAVA_TEMURIN_API_BASE_URL";
const JAVA_TEMURIN_FEATURE_RELEASE_PAGE_LIMIT: u32 = 100;
const NODE_RELEASE_METADATA_ENV: &str = "DEVENV_NODE_RELEASE_METADATA";
const NODE_OFFICIAL_RELEASE_INDEX_ENV: &str = "DEVENV_NODE_OFFICIAL_RELEASE_INDEX";
const NODE_OFFICIAL_SHASUMS_DIR_ENV: &str = "DEVENV_NODE_OFFICIAL_SHASUMS_DIR";
const NODE_OFFICIAL_BASE_URL_ENV: &str = "DEVENV_NODE_OFFICIAL_BASE_URL";
const PYTHON_RELEASE_METADATA_ENV: &str = "DEVENV_PYTHON_RELEASE_METADATA";
const DEVENV_ENABLE_CATALOG_ENV: &str = "DEVENV_ENABLE_CATALOG";
const DEVENV_CATALOG_BASE_URL_ENV: &str = "DEVENV_CATALOG_BASE_URL";
const METADATA_CACHE_TTL_SECONDS: u64 = 24 * 60 * 60;

pub fn run<I, S, O, E>(args: I, stdout: &mut O, stderr: &mut E) -> i32
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
    O: Write,
    E: Write,
{
    let args = args
        .into_iter()
        .map(|arg| arg.as_ref().to_owned())
        .collect::<Vec<_>>();
    let context = CommandContext::from_env();

    match dispatch(args, stdout, stderr, &context) {
        Ok(exit_code) => exit_code,
        Err(error) => {
            let _ = writeln!(stderr, "{error}");
            error.exit_code()
        }
    }
}

fn dispatch<O, E>(
    args: Vec<String>,
    stdout: &mut O,
    stderr: &mut E,
    context: &CommandContext,
) -> Result<i32, CliError>
where
    O: Write,
    E: Write,
{
    match args.first().map(String::as_str) {
        None => Err(CliError::usage(format!(
            "missing command\ntry `{COMMAND_NAME} doctor`, `{COMMAND_NAME} --help`, or `{COMMAND_NAME} --version`"
        ))),
        Some("--help" | "-h") => {
            write_global_help(stdout)?;
            Ok(0)
        }
        Some("--version" | "-V") => {
            write_version(stdout)?;
            Ok(0)
        }
        Some("help") => {
            run_help_command(&args[1..], stdout)?;
            Ok(0)
        }
        Some("doctor") => run_doctor_command(&args[1..], stdout, context),
        Some("local") => {
            run_scope_command(ScopeCommand::Local, &args[1..], stdout, context)?;
            Ok(0)
        }
        Some("global") => {
            run_scope_command(ScopeCommand::Global, &args[1..], stdout, context)?;
            Ok(0)
        }
        Some("shell") => {
            run_scope_command(ScopeCommand::Shell, &args[1..], stdout, context)?;
            Ok(0)
        }
        Some("use") => {
            run_use_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("current") => {
            run_current_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("add") => {
            run_add_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("remove") => {
            run_remove_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("list") => {
            run_list_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("list-remote") => {
            run_list_remote_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("metadata") => {
            run_metadata_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("provider") => {
            run_provider_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("install") => {
            run_install_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("uninstall") => {
            run_uninstall_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("shim") => run_shim_command(&args[1..], stdout, stderr, context),
        Some("activate") => {
            run_activate_command(&args[1..], stdout, context)?;
            Ok(0)
        }
        Some("exec") => run_exec_command(&args[1..], stdout, stderr, context),
        Some(arg) => Err(CliError::usage(format!(
            "unknown command `{arg}`\ntry `{COMMAND_NAME} doctor`, `{COMMAND_NAME} --help`, or `{COMMAND_NAME} --version`"
        ))),
    }
}

fn run_help_command<O>(args: &[String], stdout: &mut O) -> Result<(), CliError>
where
    O: Write,
{
    match args {
        [] => {
            write_global_help(stdout)?;
            Ok(())
        }
        [command] => write_command_help(command, stdout),
        _ => Err(CliError::usage("usage: devenv help [command]".to_owned())),
    }
}

fn write_global_help<O>(stdout: &mut O) -> io::Result<()>
where
    O: Write,
{
    writeln!(
        stdout,
        r#"DevEnv - CLI development environment manager

Usage: devenv <command> [args]

Commands:
  add           Register an existing runtime that DevEnv does not own.
  remove        Remove an external runtime registration without deleting files.
  install       Install a runtime into DevEnv-owned storage.
  uninstall     Delete a runtime from DevEnv-owned storage.
  local         Select a project-local tool version.
  global        Select a global tool version.
  shell         Print shell-scoped selection exports.
  use           Select a tool version with an explicit scope.
  current       Show selected tool versions.
  list          List installed, registered, and discovered runtimes.
  list-remote   List installable versions from configured metadata.
  metadata      Inspect and update remote metadata cache state.
  provider      Show built-in provider capabilities.
  exec          Run a command with selected tool environments.
  activate      Print shell activation for DevEnv shims.
  shim          Manage and dispatch shims.
  doctor        Check DevEnv home, registry, installs, and shims.
  help          Show this help or command-specific help.

Supported tools:
  java, go, node, python, ruby, php, rust, flutter, terraform, opentofu

Examples:
  devenv add java /path/to/jdk
  devenv local java 17
  devenv install go 1.22
  devenv uninstall go 1.22
  devenv exec -- go version
  eval "$(devenv activate zsh)"

Run `devenv help <command>` for command-specific usage."#
    )
}

fn write_command_help<O>(command: &str, stdout: &mut O) -> Result<(), CliError>
where
    O: Write,
{
    let Some(help) = command_help(command) else {
        return Err(CliError::usage(format!(
            "unknown help topic `{command}`\ntry `{COMMAND_NAME} --help`"
        )));
    };
    writeln!(stdout, "{help}")?;
    Ok(())
}

fn command_help(command: &str) -> Option<&'static str> {
    match command {
        "add" => Some(
            r#"Usage: devenv add <tool> <path>

Registers an existing runtime. DevEnv stores a reference only and does not own or delete the runtime directory.

Example:
  devenv add java /Library/Java/JavaVirtualMachines/temurin-17.jdk/Contents/Home"#,
        ),
        "remove" => Some(
            r#"Usage: devenv remove <tool> <path>
       devenv remove <tool-selector> [path]

Removes external runtime registrations. Runtime files are not deleted.
Use the compact selector form, such as java@17, when removing by registered version instead of by path.

Example:
  devenv remove go /usr/local/go"#,
        ),
        "install" => Some(
            r#"Usage: devenv install <tool> <version> [provider-or-channel] [--dry-run]

Installs a runtime into DevEnv-owned storage. Remote metadata must be configured for installable tools.
Use --dry-run to resolve the artifact and install path without downloading or writing install state.
For Java, --distribution selects the JDK distribution. The current direct provider is temurin.
For Flutter, --channel currently accepts stable only.
The compact selector form, such as java@21, is also accepted for compatibility.

Example:
  devenv install go 1.22
  devenv install java 21 temurin --dry-run
  devenv install java 21 --dry-run --distribution temurin"#,
        ),
        "uninstall" => Some(
            r#"Usage: devenv uninstall <tool> <version>

Deletes only DevEnv-owned installs for the current platform. External registrations created with `devenv add` are not deleted.
The compact selector form, such as java@11.0.24-temurin, is also accepted for compatibility.

Example:
  devenv uninstall go 1.22
  devenv uninstall java 11.0.24-temurin"#,
        ),
        "local" => Some(
            r#"Usage: devenv local <tool> <version> [--dry-run]

Writes a project-local selection to devenv.toml in the current directory.
The compact selector form, such as java@17, is also accepted for compatibility.

Example:
  devenv local java 17"#,
        ),
        "global" => Some(
            r#"Usage: devenv global <tool> <version> [--dry-run]

Writes a global selection to DEVENV_GLOBAL_CONFIG when set, or to the default DevEnv global config under DEVENV_HOME.
The compact selector form, such as node@20, is also accepted for compatibility.

Example:
  devenv global node 20"#,
        ),
        "shell" => Some(
            r#"Usage: devenv shell <tool> <version> [--dry-run]

Prints shell-scoped environment exports for one selected tool.
The compact selector form, such as python@3.12, is also accepted for compatibility.

Example:
  eval "$(devenv shell python 3.12)""#,
        ),
        "use" => Some(
            r#"Usage: devenv use <tool> <version> [--scope local|global|shell] [--dry-run]

Selects a tool version. The default scope is local.
The compact selector form, such as ruby@3.3, is also accepted for compatibility.

Example:
  devenv use ruby 3.3 --scope local"#,
        ),
        "current" => Some(
            r#"Usage: devenv current [<tool> [version]]

Shows selected versions after applying CLI, shell, project, and global precedence.
Passing a tool and version treats it as a CLI override. The compact selector form, such as java@21, is also accepted for compatibility.

Example:
  devenv current
  devenv current java
  devenv current java 21"#,
        ),
        "list" => Some(
            r#"Usage: devenv list <tool>

Lists DevEnv-owned installs, external registrations, and configured candidate runtimes.

Example:
  devenv list go"#,
        ),
        "list-remote" => Some(
            r#"Usage: devenv list-remote <tool> [--refresh] [--offline] [--distribution temurin] [--channel stable]

Lists installable versions from configured release metadata.
For Go, --refresh downloads official metadata into the cache and --offline reads only local metadata.
For Java, --distribution selects the JDK distribution. The current direct provider is temurin.
For Flutter, --channel currently accepts stable only.

Example:
  devenv list-remote go --refresh
  devenv list-remote java --distribution temurin
  devenv list-remote flutter --channel stable"#,
        ),
        "metadata" => Some(
            r#"Usage: devenv metadata <status|update|verify-catalog> [args]

Subcommands:
  status [tool]              Show provider capability and cache status.
  update <tool|--all> [--source auto|env|cache|catalog|official]
                             Refresh cache from provider metadata or configured fixture sources.
  verify-catalog [tool] [--catalog <path-or-url>] [--source catalog|file]
                             Verify a local or configured catalog manifest and payloads.

Examples:
  devenv metadata status
  devenv metadata status go
  devenv metadata update go
  devenv metadata update go --source catalog
  devenv metadata update --all
  devenv metadata verify-catalog go --catalog ./v1 --source file"#,
        ),
        "provider" => Some(
            r#"Usage: devenv provider <list|info> [args]

Subcommands:
  list                   Show all built-in provider capabilities.
  info <tool> [provider] Show detailed provider capability for one tool or provider.

Examples:
  devenv provider list
  devenv provider info java
  devenv provider info java temurin"#,
        ),
        "exec" => Some(
            r#"Usage: devenv exec -- <command> [args...]

Runs a command with all selected tool activation plans applied.

Example:
  devenv exec -- go test ./..."#,
        ),
        "activate" => Some(
            r#"Usage: devenv activate <zsh|bash|fish|powershell>

Prints shell activation that sets DEVENV_HOME and prepends the shim directory.
Activation also generates DevEnv shims, so direct commands such as `java --version` use the current DevEnv selection after eval.
For new terminal sessions, add the exact "new sessions:" line printed by selection commands such as `devenv global` to your shell profile.

Example:
  eval "$(devenv activate zsh)"
  # For new zsh sessions, use the line printed by `devenv global <tool> <version>`"#,
        ),
        "shim" => Some(
            r#"Usage: devenv shim <init|rehash|dispatch>

Subcommands:
  init                         Generate shims for built-in tool binaries.
  rehash                       Regenerate shims after adapter metadata changes.
  dispatch <binary> -- [args]  Internal shim entrypoint.

Example:
  devenv shim init"#,
        ),
        "doctor" => Some(
            r#"Usage: devenv doctor [--json]

Checks DevEnv home, install store, runtime registry, shim directory, and config discovery.

Example:
  devenv doctor
  devenv doctor --json"#,
        ),
        "help" => Some(
            r#"Usage: devenv help [command]

Shows top-level help or command-specific help.

Example:
  devenv help install"#,
        ),
        _ => None,
    }
}

fn is_help_request(args: &[String]) -> bool {
    matches!(args, [flag] if flag == "--help" || flag == "-h")
}

fn write_version<O>(stdout: &mut O) -> io::Result<()>
where
    O: Write,
{
    writeln!(
        stdout,
        "{} {} (target={}, profile={}, git={})",
        COMMAND_NAME,
        env!("CARGO_PKG_VERSION"),
        env!("DEVENV_BUILD_TARGET"),
        env!("DEVENV_BUILD_PROFILE"),
        env!("DEVENV_BUILD_GIT_SHA")
    )
}

pub fn run_from_env() -> i32 {
    let stdout = io::stdout();
    let stderr = io::stderr();
    run(
        std::env::args().skip(1),
        &mut stdout.lock(),
        &mut stderr.lock(),
    )
}

fn run_doctor_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<i32, CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("doctor", stdout)?;
        return Ok(0);
    }

    let output_json = match args {
        [] => false,
        [flag] if flag == "--json" => true,
        _ => {
            return Err(CliError::usage("usage: devenv doctor [--json]".to_owned()));
        }
    };

    let report = DoctorReport::collect(context);
    if output_json {
        writeln!(stdout, "{}", report.to_json())?;
    } else {
        report.write_text(stdout)?;
    }

    Ok(if report.has_errors() { 1 } else { 0 })
}

fn run_scope_command<O>(
    scope: ScopeCommand,
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help(scope.command_name(), stdout)?;
        return Ok(());
    }

    let (spec, dry_run) = parse_scoped_write_args(scope.command_name(), args)?;
    apply_scope(scope, spec, dry_run, stdout, context)
}

fn run_use_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("use", stdout)?;
        return Ok(());
    }

    let (spec, scope, dry_run) = parse_use_args(args)?;
    apply_scope(scope, spec, dry_run, stdout, context)
}

fn run_current_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("current", stdout)?;
        return Ok(());
    }

    if args.len() > 2 {
        return Err(CliError::usage(
            "usage: devenv current [<tool> [version]]".to_owned(),
        ));
    }

    let project_config = discover_project_config_from(&context.current_dir)?;
    let global_config = read_global_config(context)?;

    match args {
        [selector] if selector.contains('@') => {
            let spec = parse_tool_spec(selector)?;
            let resolved = resolve_single_selection(
                spec.tool().clone(),
                Some(spec.requirement().clone()),
                project_config.as_ref(),
                global_config.as_ref(),
                context,
            )?
            .ok_or_else(|| missing_selection_error(spec.tool()))?;
            write_selection(stdout, &resolved)?;
            Ok(())
        }
        [tool, version] => {
            let spec_arg = format!("{tool}@{version}");
            let spec = parse_tool_spec(&spec_arg)?;
            let resolved = resolve_single_selection(
                spec.tool().clone(),
                Some(spec.requirement().clone()),
                project_config.as_ref(),
                global_config.as_ref(),
                context,
            )?
            .ok_or_else(|| missing_selection_error(spec.tool()))?;
            write_selection(stdout, &resolved)?;
            Ok(())
        }
        [selector] => {
            let tool = ToolName::new(selector).map_err(CoreError::from)?;
            let resolved = resolve_single_selection(
                tool.clone(),
                None,
                project_config.as_ref(),
                global_config.as_ref(),
                context,
            )?
            .ok_or_else(|| missing_selection_error(&tool))?;
            write_selection(stdout, &resolved)?;
            Ok(())
        }
        [] => {
            let tools = collect_configured_tools(
                project_config.as_ref(),
                global_config.as_ref(),
                context.env_vars.iter(),
            );
            if tools.is_empty() {
                return Err(CliError::runtime(
                    "no versions selected. Run `devenv local java 17`, `devenv global go 1.22.5`, or `eval \"$(devenv shell java 17)\"`.",
                ));
            }

            let mut wrote_any = false;
            for tool in tools {
                if let Some(resolved) = resolve_single_selection(
                    tool,
                    None,
                    project_config.as_ref(),
                    global_config.as_ref(),
                    context,
                )? {
                    write_selection(stdout, &resolved)?;
                    wrote_any = true;
                }
            }

            if wrote_any {
                Ok(())
            } else {
                Err(CliError::runtime(
                    "no versions selected. Run `devenv local java 17`, `devenv global go 1.22.5`, or `eval \"$(devenv shell java 17)\"`.",
                ))
            }
        }
        _ => Err(CliError::usage(
            "usage: devenv current [<tool> [version]]".to_owned(),
        )),
    }
}

fn run_exec_command<O, E>(
    args: &[String],
    stdout: &mut O,
    stderr: &mut E,
    context: &CommandContext,
) -> Result<i32, CliError>
where
    O: Write,
    E: Write,
{
    if is_help_request(args) {
        write_command_help("exec", stdout)?;
        return Ok(0);
    }

    let command_args = parse_exec_args(args)?;
    let project_config = discover_project_config_from(&context.current_dir)?;
    let global_config = read_global_config(context)?;
    let selections =
        resolve_all_selected_tools(project_config.as_ref(), global_config.as_ref(), context)?;

    if selections.is_empty() {
        return Err(CliError::runtime(
            "no versions selected. Run `devenv local java 17`, `devenv global go 1.22.5`, or `eval \"$(devenv shell java 17)\"` before `devenv exec -- <command>`.",
        ));
    }

    let platform = current_platform();
    let mut activation = ActivationPlan::new();

    for selection in selections {
        let plan = activation_plan_for_cli_selection(&selection, platform, context)?;
        activation = activation.extend(plan);
    }

    let exec = ExecCommand::new(command_args[0].clone(), activation)
        .with_args(command_args.iter().skip(1).cloned())
        .with_cwd(context.current_dir.clone());
    let mut runner = ProcessCommandRunner;
    let output = exec.execute(&context.env_vars, &mut runner)?;

    write!(stdout, "{}", output.stdout())?;
    write!(stderr, "{}", output.stderr())?;

    Ok(output.status_code())
}

fn run_shim_command<O, E>(
    args: &[String],
    stdout: &mut O,
    stderr: &mut E,
    context: &CommandContext,
) -> Result<i32, CliError>
where
    O: Write,
    E: Write,
{
    if is_help_request(args) {
        write_command_help("shim", stdout)?;
        return Ok(0);
    }

    match args.first().map(String::as_str) {
        Some("init") => {
            if args.len() != 1 {
                return Err(CliError::usage("usage: devenv shim init".to_owned()));
            }
            let (shim_dir, count) = run_shim_rehash(context)?;
            writeln!(stdout, "initialized shims {}", shim_dir.display())?;
            writeln!(stdout, "shims: {count}")?;
            Ok(0)
        }
        Some("rehash") => {
            if args.len() != 1 {
                return Err(CliError::usage("usage: devenv shim rehash".to_owned()));
            }
            let (shim_dir, count) = run_shim_rehash(context)?;
            writeln!(stdout, "generated {count} shims {}", shim_dir.display())?;
            Ok(0)
        }
        Some("dispatch") => run_shim_dispatch(&args[1..], stdout, stderr, context),
        _ => Err(CliError::usage(
            "usage: devenv shim <init|rehash|dispatch>".to_owned(),
        )),
    }
}

fn run_shim_rehash(context: &CommandContext) -> Result<(PathBuf, usize), CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;

    let adapters = builtin_shim_adapters();
    let adapter_refs = adapter_refs(&adapters);
    let shim_dir = home.shims_dir();
    let mut writer =
        FileShimWriter::new(&shim_dir).with_dispatch_command(current_executable_command());
    let specs = rehash_shims(&adapter_refs, &mut writer)?;

    Ok((shim_dir, specs.len()))
}

fn run_shim_dispatch<O, E>(
    args: &[String],
    stdout: &mut O,
    stderr: &mut E,
    context: &CommandContext,
) -> Result<i32, CliError>
where
    O: Write,
    E: Write,
{
    let (binary_name, command_args) = parse_shim_dispatch_args(args)?;
    let adapters = builtin_shim_adapters();
    let adapter_refs = adapter_refs(&adapters);
    let tool = tool_for_shim_binary(binary_name, &adapter_refs)?.ok_or_else(|| {
        CliError::runtime(format!(
            "`{binary_name}` is not managed by DevEnv shims. Run `devenv shim rehash` to generate supported shims."
        ))
    })?;

    let project_config = discover_project_config_from(&context.current_dir)?;
    let global_config = read_global_config(context)?;
    let selection = resolve_single_selection(
        tool.clone(),
        None,
        project_config.as_ref(),
        global_config.as_ref(),
        context,
    )?
    .ok_or_else(|| missing_selection_error(&tool))?;
    let activation = activation_plan_for_cli_selection(&selection, current_platform(), context)?;
    let mut runner = ProcessCommandRunner;
    let output = dispatch_shim_command(
        binary_name,
        command_args,
        activation,
        &context.current_dir,
        &context.env_vars,
        &mut runner,
    )?;

    write!(stdout, "{}", output.stdout())?;
    write!(stderr, "{}", output.stderr())?;

    Ok(output.status_code())
}

fn run_add_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("add", stdout)?;
        return Ok(());
    }

    if args.len() != 2 {
        return Err(CliError::usage(
            "usage: devenv add <tool> <path>".to_owned(),
        ));
    }

    let tool = ToolName::new(&args[0]).map_err(CoreError::from)?;
    match tool.as_str() {
        "java" => run_add_java(&args[1], stdout, context),
        "go" => run_add_go(&args[1], stdout, context),
        "flutter" => run_add_flutter(&args[1], stdout, context),
        "terraform" => run_add_iac(IacTool::Terraform, &args[1], stdout, context),
        "opentofu" => run_add_iac(IacTool::OpenTofu, &args[1], stdout, context),
        "node" => run_add_node(&args[1], stdout, context),
        "python" => run_add_python(&args[1], stdout, context),
        "ruby" => run_add_ruby(&args[1], stdout, context),
        "php" => run_add_php(&args[1], stdout, context),
        "rust" => run_add_rust(&args[1], stdout, context),
        _ => Err(CliError::runtime(format!(
            "`devenv add` is not implemented for `{tool}` yet"
        ))),
    }
}

fn run_remove_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("remove", stdout)?;
        return Ok(());
    }

    if args.is_empty() || args.len() > 2 {
        return Err(CliError::usage(
            "usage: devenv remove <tool> <path>\n       devenv remove <tool-selector> [path]"
                .to_owned(),
        ));
    }

    if args[0] == "java" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove java <path>".to_owned(),
            ));
        }
        return run_remove_java_path(&args[1], stdout, context);
    }
    if args[0] == "go" {
        if args.len() != 2 {
            return Err(CliError::usage("usage: devenv remove go <path>".to_owned()));
        }
        return run_remove_go_path(&args[1], stdout, context);
    }
    if args[0] == "node" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove node <path>".to_owned(),
            ));
        }
        return run_remove_node_path(&args[1], stdout, context);
    }
    if args[0] == "flutter" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove flutter <path>".to_owned(),
            ));
        }
        return run_remove_flutter_path(&args[1], stdout, context);
    }
    if args[0] == "terraform" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove terraform <path>".to_owned(),
            ));
        }
        return run_remove_iac_path(IacTool::Terraform, &args[1], stdout, context);
    }
    if args[0] == "opentofu" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove opentofu <path>".to_owned(),
            ));
        }
        return run_remove_iac_path(IacTool::OpenTofu, &args[1], stdout, context);
    }
    if args[0] == "python" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove python <path>".to_owned(),
            ));
        }
        return run_remove_python_path(&args[1], stdout, context);
    }
    if args[0] == "ruby" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove ruby <path>".to_owned(),
            ));
        }
        return run_remove_ruby_path(&args[1], stdout, context);
    }
    if args[0] == "php" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove php <path>".to_owned(),
            ));
        }
        return run_remove_php_path(&args[1], stdout, context);
    }
    if args[0] == "rust" {
        if args.len() != 2 {
            return Err(CliError::usage(
                "usage: devenv remove rust <path>".to_owned(),
            ));
        }
        return run_remove_rust_path(&args[1], stdout, context);
    }

    let spec = parse_tool_spec(&args[0])?;
    let root = args
        .get(1)
        .map(|path| resolve_input_path(path, &context.current_dir));
    match spec.tool().as_str() {
        "java" => {
            let version = if let Some(root) = &root {
                validate_jdk_home(root)?.version().raw().to_owned()
            } else {
                spec.requirement().raw().to_owned()
            };
            run_remove_java(&version, root.as_deref(), stdout, context)
        }
        "go" => {
            let version = if let Some(root) = &root {
                validate_go_sdk_home(root)?.version().raw().to_owned()
            } else {
                spec.requirement().raw().to_owned()
            };
            run_remove_go(&version, root.as_deref(), stdout, context)
        }
        "node" => {
            let version = if let Some(root) = &root {
                validate_node_home(root)?.version().raw().to_owned()
            } else {
                spec.requirement().raw().to_owned()
            };
            run_remove_node(&version, root.as_deref(), stdout, context)
        }
        "flutter" => {
            let (version, canonical_root) = if let Some(root) = &root {
                let runtime = validate_flutter_sdk_home(root)?;
                (
                    runtime.version().raw().to_owned(),
                    Some(runtime.root().to_path_buf()),
                )
            } else {
                (spec.requirement().raw().to_owned(), None)
            };
            run_remove_flutter(&version, canonical_root.as_deref(), stdout, context)
        }
        "terraform" => {
            let (version, canonical_root) = if let Some(root) = &root {
                let runtime = validate_iac_tool_home(root, IacTool::Terraform)?;
                (
                    runtime.version().raw().to_owned(),
                    Some(runtime.root().to_path_buf()),
                )
            } else {
                (spec.requirement().raw().to_owned(), None)
            };
            run_remove_iac(
                IacTool::Terraform,
                &version,
                canonical_root.as_deref(),
                stdout,
                context,
            )
        }
        "opentofu" => {
            let (version, canonical_root) = if let Some(root) = &root {
                let runtime = validate_iac_tool_home(root, IacTool::OpenTofu)?;
                (
                    runtime.version().raw().to_owned(),
                    Some(runtime.root().to_path_buf()),
                )
            } else {
                (spec.requirement().raw().to_owned(), None)
            };
            run_remove_iac(
                IacTool::OpenTofu,
                &version,
                canonical_root.as_deref(),
                stdout,
                context,
            )
        }
        "python" => {
            let version = if let Some(root) = &root {
                validate_python_home(root)?.version().raw().to_owned()
            } else {
                spec.requirement().raw().to_owned()
            };
            run_remove_python(&version, root.as_deref(), stdout, context)
        }
        "ruby" => {
            let version = if let Some(root) = &root {
                validate_ruby_home(root)?.version().raw().to_owned()
            } else {
                spec.requirement().raw().to_owned()
            };
            run_remove_ruby(&version, root.as_deref(), stdout, context)
        }
        "php" => {
            let version = if let Some(root) = &root {
                validate_php_home(root)?.version().raw().to_owned()
            } else {
                spec.requirement().raw().to_owned()
            };
            run_remove_php(&version, root.as_deref(), stdout, context)
        }
        "rust" => {
            let version = if let Some(root) = &root {
                validate_rust_toolchain_home(root)?
                    .version()
                    .raw()
                    .to_owned()
            } else {
                spec.requirement().raw().to_owned()
            };
            run_remove_rust(&version, root.as_deref(), stdout, context)
        }
        _ => Err(CliError::runtime(format!(
            "`devenv remove` is not implemented for `{}` yet",
            spec.tool()
        ))),
    }
}

fn run_list_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("list", stdout)?;
        return Ok(());
    }

    if args.len() != 1 {
        return Err(CliError::usage("usage: devenv list <tool>".to_owned()));
    }

    let tool = ToolName::new(&args[0]).map_err(CoreError::from)?;
    match tool.as_str() {
        "java" => run_list_java(stdout, context),
        "go" => run_list_go(stdout, context),
        "flutter" => run_list_flutter(stdout, context),
        "terraform" => run_list_iac(IacTool::Terraform, stdout, context),
        "opentofu" => run_list_iac(IacTool::OpenTofu, stdout, context),
        "node" => run_list_node(stdout, context),
        "python" => run_list_python(stdout, context),
        "ruby" => run_list_ruby(stdout, context),
        "php" => run_list_php(stdout, context),
        "rust" => run_list_rust(stdout, context),
        _ => Err(CliError::runtime(format!(
            "`devenv list` is not implemented for `{tool}` yet"
        ))),
    }
}

fn run_list_remote_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("list-remote", stdout)?;
        return Ok(());
    }

    let (tool_arg, options) = parse_list_remote_args(args)?;
    let tool = ToolName::new(tool_arg).map_err(CoreError::from)?;
    match tool.as_str() {
        "java" => {
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            let distribution = java_distribution_from_option(options.distribution.as_deref())?;
            ensure_supported_java_distribution(&distribution)?;
            let source = JavaReleaseVersionSource::with_distribution(
                load_java_release_metadata_with_options(context, &distribution, options)?,
                distribution,
            );
            write_remote_versions(&tool, &source, Some(source.distribution().as_str()), stdout)
        }
        "go" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            let source = GoReleaseVersionSource::new(load_go_release_metadata_with_options(
                context, options,
            )?);
            write_remote_versions(&tool, &source, None, stdout)
        }
        "flutter" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            ensure_supported_flutter_channel(options.channel.as_deref())?;
            let source = FlutterReleaseVersionSource::new(
                load_flutter_release_metadata_with_options(context, options)?,
            );
            write_remote_versions(&tool, &source, Some("stable"), stdout)
        }
        "terraform" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            let source = IacReleaseVersionSource::new(
                IacTool::Terraform,
                load_iac_release_metadata_with_options(IacTool::Terraform, context, options)?,
            );
            write_remote_versions(&tool, &source, None, stdout)
        }
        "opentofu" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            let source = IacReleaseVersionSource::new(
                IacTool::OpenTofu,
                load_iac_release_metadata_with_options(IacTool::OpenTofu, context, options)?,
            );
            write_remote_versions(&tool, &source, None, stdout)
        }
        "node" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            let source = NodeReleaseVersionSource::new(load_node_release_metadata_with_options(
                context, options,
            )?);
            write_remote_versions(&tool, &source, None, stdout)
        }
        "python" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            if context.env_vars.contains_key(PYTHON_RELEASE_METADATA_ENV) {
                let source =
                    PythonReleaseVersionSource::new(load_python_release_metadata(context)?);
                write_remote_versions(&tool, &source, Some("cpython"), stdout)
            } else {
                write_manifest_known_remote_versions(
                    &tool,
                    include_str!("../../../metadata/providers/python/cpython/manifest.json"),
                    Some("cpython"),
                    stdout,
                )
            }
        }
        "rust" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            write_manifest_known_remote_versions(
                &tool,
                include_str!("../../../metadata/providers/rust/rustup/manifest.json"),
                Some("rustup"),
                stdout,
            )
        }
        "ruby" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            write_manifest_known_remote_versions(
                &tool,
                include_str!("../../../metadata/providers/ruby/local/manifest.json"),
                Some("local"),
                stdout,
            )
        }
        "php" => {
            reject_java_distribution_option_for_tool(&tool, options.distribution.as_deref())?;
            reject_flutter_channel_option_for_tool(&tool, options.channel.as_deref())?;
            write_manifest_known_remote_versions(
                &tool,
                include_str!("../../../metadata/providers/php/local/manifest.json"),
                Some("local"),
                stdout,
            )
        }
        _ => Err(unsupported_list_remote_error(&tool)),
    }
}

#[derive(Debug, Clone, Default)]
struct ListRemoteOptions {
    refresh: bool,
    offline: bool,
    distribution: Option<String>,
    channel: Option<String>,
}

fn parse_list_remote_args(args: &[String]) -> Result<(&str, ListRemoteOptions), CliError> {
    if args.is_empty() {
        return Err(CliError::usage(
            "usage: devenv list-remote <tool> [--refresh] [--offline] [--distribution temurin] [--channel stable]"
                .to_owned(),
        ));
    }

    let mut tool = None;
    let mut options = ListRemoteOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--refresh" => {
                options.refresh = true;
                index += 1;
            }
            "--offline" => {
                options.offline = true;
                index += 1;
            }
            "--distribution" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliError::usage(
                        "`--distribution` requires a value\nusage: devenv list-remote <tool> [--refresh] [--offline] [--distribution temurin] [--channel stable]"
                            .to_owned(),
                    ));
                };
                options.distribution = Some(value.clone());
                index += 2;
            }
            "--channel" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliError::usage(
                        "`--channel` requires a value\nusage: devenv list-remote <tool> [--refresh] [--offline] [--distribution temurin] [--channel stable]"
                            .to_owned(),
                    ));
                };
                options.channel = Some(value.clone());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::usage(format!(
                    "unknown list-remote option `{value}`\nusage: devenv list-remote <tool> [--refresh] [--offline] [--distribution temurin] [--channel stable]"
                )));
            }
            value => {
                if tool.replace(value).is_some() {
                    return Err(CliError::usage(
                        "usage: devenv list-remote <tool> [--refresh] [--offline] [--distribution temurin] [--channel stable]"
                            .to_owned(),
                    ));
                }
                index += 1;
            }
        }
    }

    if options.refresh && options.offline {
        return Err(CliError::usage(
            "`--refresh` cannot be combined with `--offline`".to_owned(),
        ));
    }

    let Some(tool) = tool else {
        return Err(CliError::usage(
            "usage: devenv list-remote <tool> [--refresh] [--offline] [--distribution temurin] [--channel stable]"
                .to_owned(),
        ));
    };

    Ok((tool, options))
}

fn reject_java_distribution_option_for_tool(
    tool: &ToolName,
    distribution: Option<&str>,
) -> Result<(), CliError> {
    if let Some(distribution) = distribution {
        return Err(CliError::runtime(format!(
            "`--distribution {distribution}` is only supported for Java; `{tool}` does not use Java distributions"
        )));
    }
    Ok(())
}

fn reject_flutter_channel_option_for_tool(
    tool: &ToolName,
    channel: Option<&str>,
) -> Result<(), CliError> {
    if let Some(channel) = channel {
        return Err(CliError::runtime(format!(
            "`--channel {channel}` is only supported for Flutter; `{tool}` does not use Flutter channels"
        )));
    }
    Ok(())
}

fn ensure_supported_flutter_channel(channel: Option<&str>) -> Result<(), CliError> {
    let channel = channel.unwrap_or("stable");
    if channel == "stable" {
        return Ok(());
    }

    let tool = ToolName::new("flutter").map_err(CoreError::from)?;
    Err(unsupported_selector_error(
        &tool,
        ProviderSelectorDimension::Channel,
        channel,
    ))
}

fn java_distribution_from_option(value: Option<&str>) -> Result<JavaDistribution, CliError> {
    value
        .map(JavaDistribution::named)
        .transpose()
        .map_err(CliError::from)
        .map(|distribution| distribution.unwrap_or_default())
}

fn ensure_supported_java_distribution(distribution: &JavaDistribution) -> Result<(), CliError> {
    if distribution.as_str() == "temurin" {
        return Ok(());
    }

    let tool = ToolName::new("java").map_err(CoreError::from)?;
    Err(unsupported_selector_error(
        &tool,
        ProviderSelectorDimension::Distribution,
        distribution.as_str(),
    ))
}

fn unsupported_selector_error(
    tool: &ToolName,
    dimension: ProviderSelectorDimension,
    value: &str,
) -> CliError {
    let supported = supported_selector_values(tool, dimension);
    let supported = if supported.is_empty() {
        "-".to_owned()
    } else {
        supported.join(", ")
    };
    let label = dimension.as_str();
    CliError::runtime(format!(
        "unsupported {label} `{value}` for {tool}; supported {label}s: {supported}. If this is not a typo, that provider is not implemented yet.\nnext: run `devenv provider info {tool}`"
    ))
}

fn supported_selector_values(tool: &ToolName, dimension: ProviderSelectorDimension) -> Vec<String> {
    builtin_provider_registry()
        .providers_for_tool(tool)
        .into_iter()
        .filter(|capability| capability.supports_selector_dimension(dimension))
        .map(|capability| capability.provider().as_str().to_owned())
        .collect()
}

fn write_remote_versions<O>(
    tool: &ToolName,
    source: &dyn VersionSource,
    suffix: Option<&str>,
    stdout: &mut O,
) -> Result<(), CliError>
where
    O: Write,
{
    for version in list_remote_versions(tool, source)? {
        if let Some(suffix) = suffix {
            writeln!(stdout, "{tool} {version} {suffix}")?;
        } else {
            writeln!(stdout, "{tool} {version}")?;
        }
    }
    Ok(())
}

fn write_manifest_known_remote_versions<O>(
    tool: &ToolName,
    manifest_json: &str,
    suffix: Option<&str>,
    stdout: &mut O,
) -> Result<(), CliError>
where
    O: Write,
{
    let manifest = serde_json::from_str::<serde_json::Value>(manifest_json).map_err(|error| {
        CliError::runtime(format!(
            "failed to parse built-in provider manifest for {tool}: {error}"
        ))
    })?;
    let versions = manifest
        .get("version")
        .and_then(serde_json::Value::as_object)
        .and_then(|version| version.get("known_versions"))
        .and_then(serde_json::Value::as_object)
        .and_then(|known_versions| known_versions.get("versions"))
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            CliError::runtime(format!(
                "provider manifest for {tool} does not contain `version.known_versions.versions`"
            ))
        })?;

    if versions.is_empty() {
        return Err(CliError::runtime(format!(
            "provider manifest for {tool} does not contain any known versions"
        )));
    }

    for version in versions {
        let version = version.as_str().ok_or_else(|| {
            CliError::runtime(format!(
                "provider manifest for {tool} contains a non-string known version"
            ))
        })?;
        if let Some(suffix) = suffix {
            writeln!(stdout, "{tool} {version} {suffix}")?;
        } else {
            writeln!(stdout, "{tool} {version}")?;
        }
    }

    Ok(())
}

fn run_metadata_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("metadata", stdout)?;
        return Ok(());
    }

    match args.first().map(String::as_str) {
        Some("status") => run_metadata_status_command(&args[1..], stdout, context),
        Some("update") => run_metadata_update_command(&args[1..], stdout, context),
        Some("verify-catalog") => run_metadata_verify_catalog_command(&args[1..], stdout, context),
        Some(command) => Err(CliError::usage(format!(
            "unknown metadata command `{command}`\nusage: devenv metadata <status|update|verify-catalog> [args]"
        ))),
        None => Err(CliError::usage(
            "usage: devenv metadata <status|update|verify-catalog> [args]".to_owned(),
        )),
    }
}

fn run_metadata_status_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("metadata", stdout)?;
        return Ok(());
    }

    if args.len() > 1 {
        return Err(CliError::usage(
            "usage: devenv metadata status [tool]".to_owned(),
        ));
    }

    let registry = builtin_provider_registry();
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let cache = FileMetadataCache::at_home(&home);
    let clock = SystemClock;
    let capabilities = match args {
        [] => registry.providers().iter().collect::<Vec<_>>(),
        [tool_name] => {
            let tool = ToolName::new(tool_name).map_err(CoreError::from)?;
            let providers = registry.providers_for_tool(&tool);
            if providers.is_empty() {
                return Err(unknown_provider_tool_error(&tool, registry.providers()));
            }
            providers
        }
        _ => unreachable!("metadata status arg count is validated above"),
    };

    writeln!(stdout, "Metadata status")?;
    writeln!(
        stdout,
        "cache_root {}",
        cache.metadata_cache_dir().display()
    )?;
    for capability in capabilities {
        write_metadata_status(capability, &cache, &clock, stdout)?;
    }

    Ok(())
}

fn run_metadata_update_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("metadata", stdout)?;
        return Ok(());
    }

    let options = parse_metadata_update_args(args)?;

    let registry = builtin_provider_registry();
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut cache = FileMetadataCache::at_home(&home);
    let clock = SystemClock;

    if options.target == "--all" {
        writeln!(stdout, "Metadata update")?;
        for capability in registry.providers() {
            match update_metadata_provider(
                capability,
                &mut cache,
                &clock,
                context,
                false,
                options.source_mode,
            ) {
                Ok(MetadataUpdateOutcome::Updated) => {
                    writeln!(
                        stdout,
                        "{} {} updated cache=fresh",
                        capability.tool(),
                        capability.provider()
                    )?;
                }
                Ok(MetadataUpdateOutcome::Skipped(reason)) => {
                    writeln!(
                        stdout,
                        "{} {} skipped {}",
                        capability.tool(),
                        capability.provider(),
                        metadata_update_skipped_reason(capability, &reason)
                    )?;
                }
                Err(error) => {
                    return Err(CliError::runtime(format!(
                        "failed to update metadata for {} {}: {error}",
                        capability.tool(),
                        capability.provider()
                    )));
                }
            }
        }
        return Ok(());
    }

    let tool = ToolName::new(&options.target).map_err(CoreError::from)?;
    let providers = registry.providers_for_tool(&tool);
    if providers.is_empty() {
        return Err(unknown_provider_tool_error(&tool, registry.providers()));
    }

    let mut updated = false;
    for capability in providers {
        match update_metadata_provider(
            capability,
            &mut cache,
            &clock,
            context,
            true,
            options.source_mode,
        )? {
            MetadataUpdateOutcome::Updated => {
                updated = true;
                writeln!(
                    stdout,
                    "{} {} updated cache=fresh",
                    capability.tool(),
                    capability.provider()
                )?;
            }
            MetadataUpdateOutcome::Skipped(reason) => {
                return Err(metadata_update_skipped_error(capability, &reason));
            }
        }
    }

    if updated {
        Ok(())
    } else {
        Err(CliError::runtime(format!(
            "metadata update did not find an updatable provider for `{tool}`"
        )))
    }
}

#[derive(Debug, Clone)]
struct MetadataUpdateOptions {
    target: String,
    source_mode: MetadataSourceMode,
}

fn parse_metadata_update_args(args: &[String]) -> Result<MetadataUpdateOptions, CliError> {
    if args.is_empty() {
        return Err(CliError::usage(metadata_update_usage()));
    }

    let mut target = None;
    let mut source_mode = MetadataSourceMode::Auto;
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--source" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliError::usage(format!(
                        "`--source` requires a value\n{}",
                        metadata_update_usage()
                    )));
                };
                source_mode = parse_metadata_source_mode(value)?;
                index += 2;
            }
            value if value.starts_with('-') && value != "--all" => {
                return Err(CliError::usage(format!(
                    "unknown metadata update option `{value}`\n{}",
                    metadata_update_usage()
                )));
            }
            value => {
                if target.replace(value.to_owned()).is_some() {
                    return Err(CliError::usage(metadata_update_usage()));
                }
                index += 1;
            }
        }
    }

    let Some(target) = target else {
        return Err(CliError::usage(metadata_update_usage()));
    };

    Ok(MetadataUpdateOptions {
        target,
        source_mode,
    })
}

fn metadata_update_usage() -> String {
    "usage: devenv metadata update <tool|--all> [--source auto|env|cache|catalog|official]"
        .to_owned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MetadataVerifyCatalogSource {
    Catalog,
    File,
}

impl MetadataVerifyCatalogSource {
    fn parse(value: &str) -> Result<Self, CliError> {
        match value {
            "catalog" => Ok(Self::Catalog),
            "file" => Ok(Self::File),
            _ => Err(CliError::usage(format!(
                "unknown metadata verify-catalog source `{value}`; expected catalog or file"
            ))),
        }
    }
}

#[derive(Debug, Clone)]
struct MetadataVerifyCatalogOptions {
    tool: Option<ToolName>,
    catalog_reference: String,
    source: MetadataVerifyCatalogSource,
}

fn run_metadata_verify_catalog_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("metadata", stdout)?;
        return Ok(());
    }

    let options = parse_metadata_verify_catalog_args(args, context)?;
    let mut adapter = CatalogFetchAdapter::new(
        &options.catalog_reference,
        ReqwestMetadataHttpClient::new()?,
    )
    .map_err(CliError::from)?;
    let request = CatalogFetchRequest::new(&options.catalog_reference);
    let trust_root = builtin_catalog_trust_root();
    let mut verifier = Sha256CatalogTrustVerifier;
    let manifest_response =
        adapter.fetch_and_verify_manifest(&request, &mut verifier, &trust_root)?;
    let manifest_sha256 = format!("sha256:{}", hex_sha256(manifest_response.bytes()));
    let manifest = parse_catalog_manifest(manifest_response.bytes())?;
    let clock = SystemClock;
    validate_catalog_manifest_for_update(&manifest, &clock)?;

    let entries = manifest
        .entries()
        .iter()
        .filter(|entry| {
            options
                .tool
                .as_ref()
                .is_none_or(|tool| entry.tool() == tool)
        })
        .cloned()
        .collect::<Vec<_>>();

    if entries.is_empty() {
        let target = options
            .tool
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "any tool".to_owned());
        return Err(CliError::runtime(format!(
            "catalog manifest `{}` does not contain entries for {target}\nnext: verify the catalog version or run `devenv metadata verify-catalog` without a tool filter.",
            manifest.catalog_id()
        )));
    }

    writeln!(stdout, "Catalog verification")?;
    writeln!(stdout, "root {}", options.catalog_reference)?;
    writeln!(
        stdout,
        "source {}",
        metadata_verify_catalog_source_label(options.source)
    )?;
    writeln!(stdout, "catalog_id {}", manifest.catalog_id())?;
    writeln!(stdout, "catalog_version {}", manifest.catalog_version())?;
    writeln!(stdout, "sequence {}", manifest.sequence())?;
    writeln!(stdout, "generated_at {}", manifest.generated_at())?;
    writeln!(stdout, "expires_at {}", manifest.expires_at())?;
    writeln!(stdout, "manifest_sha256 {manifest_sha256}")?;
    writeln!(stdout, "entries {}", entries.len())?;

    for entry in entries {
        let payload = adapter.fetch_entry_payload(&entry)?;
        validate_catalog_payload_for_entry(&entry, payload.bytes())?;
        writeln!(
            stdout,
            "entry {} {} path={} payload_sha256={} ttl_seconds={} status=verified",
            entry.tool(),
            entry.provider(),
            entry.descriptor().path(),
            entry.descriptor().sha256(),
            entry.descriptor().ttl_seconds()
        )?;
    }

    Ok(())
}

fn parse_metadata_verify_catalog_args(
    args: &[String],
    context: &CommandContext,
) -> Result<MetadataVerifyCatalogOptions, CliError> {
    let mut tool = None;
    let mut catalog_reference = None;
    let mut source = MetadataVerifyCatalogSource::Catalog;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--catalog" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliError::usage(format!(
                        "`--catalog` requires a value\n{}",
                        metadata_verify_catalog_usage()
                    )));
                };
                catalog_reference = Some(catalog_reference_from_input(value, context));
                index += 2;
            }
            "--source" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliError::usage(format!(
                        "`--source` requires a value\n{}",
                        metadata_verify_catalog_usage()
                    )));
                };
                source = MetadataVerifyCatalogSource::parse(value)?;
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::usage(format!(
                    "unknown metadata verify-catalog option `{value}`\n{}",
                    metadata_verify_catalog_usage()
                )));
            }
            value => {
                if tool.is_some() {
                    return Err(CliError::usage(metadata_verify_catalog_usage()));
                }
                tool = Some(ToolName::new(value).map_err(CoreError::from)?);
                index += 1;
            }
        }
    }

    let catalog_reference = match catalog_reference {
        Some(reference) => reference,
        None => context
            .env_vars
            .get(DEVENV_CATALOG_BASE_URL_ENV)
            .cloned()
            .ok_or_else(|| {
                catalog_unavailable_error(format!(
                    "missing `{DEVENV_CATALOG_BASE_URL_ENV}` and no `--catalog` path or URL was provided"
                ))
            })?,
    };

    if source == MetadataVerifyCatalogSource::File && !catalog_reference.starts_with("file://") {
        return Err(CliError::usage(format!(
            "`--source file` requires `--catalog` or `{DEVENV_CATALOG_BASE_URL_ENV}` to resolve to a local file catalog\n{}",
            metadata_verify_catalog_usage()
        )));
    }

    Ok(MetadataVerifyCatalogOptions {
        tool,
        catalog_reference,
        source,
    })
}

fn metadata_verify_catalog_usage() -> String {
    "usage: devenv metadata verify-catalog [tool] [--catalog <path-or-url>] [--source catalog|file]"
        .to_owned()
}

fn metadata_verify_catalog_source_label(source: MetadataVerifyCatalogSource) -> &'static str {
    match source {
        MetadataVerifyCatalogSource::Catalog => "catalog",
        MetadataVerifyCatalogSource::File => "file",
    }
}

fn catalog_reference_from_input(input: &str, context: &CommandContext) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with("file://")
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
    {
        return trimmed.to_owned();
    }

    let path = resolve_input_path(trimmed, &context.current_dir);
    let absolute = path.canonicalize().unwrap_or(path);
    file_url_from_path(&absolute)
}

fn file_url_from_path(path: &Path) -> String {
    format!(
        "file://{}",
        percent_encode_file_url_path(&path.to_string_lossy())
    )
}

fn percent_encode_file_url_path(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(*byte))
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}

fn validate_catalog_payload_for_entry(entry: &CatalogEntry, bytes: &[u8]) -> Result<(), CliError> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes).map_err(|error| {
        CliError::runtime(format!(
            "failed to parse catalog payload `{}` JSON: {error}",
            entry.descriptor().path()
        ))
    })?;
    let schema_version = required_json_u32(&value, "schema_version", "catalog payload")?;
    if schema_version != CATALOG_MANIFEST_SCHEMA_VERSION {
        return Err(CliError::from(CoreError::catalog_trust(
            CatalogTrustFailure::UnsupportedSchemaVersion {
                expected: CATALOG_MANIFEST_SCHEMA_VERSION,
                actual: schema_version,
            },
        )));
    }
    let tool = required_json_string(&value, "tool", "catalog payload")?;
    let provider = required_json_string(&value, "provider", "catalog payload")?;
    if tool != entry.tool().as_str() || provider != entry.provider().as_str() {
        return Err(CliError::runtime(format!(
            "catalog payload `{}` declares {tool}/{provider}, but manifest entry is {}/{}",
            entry.descriptor().path(),
            entry.tool(),
            entry.provider()
        )));
    }
    Ok(())
}

fn run_provider_command<O>(
    args: &[String],
    stdout: &mut O,
    _context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("provider", stdout)?;
        return Ok(());
    }

    match args.first().map(String::as_str) {
        Some("list") => run_provider_list_command(&args[1..], stdout),
        Some("info") => run_provider_info_command(&args[1..], stdout),
        Some(command) => Err(CliError::usage(format!(
            "unknown provider command `{command}`\nusage: devenv provider <list|info> [args]"
        ))),
        None => Err(CliError::usage(
            "usage: devenv provider <list|info> [args]".to_owned(),
        )),
    }
}

fn run_provider_list_command<O>(args: &[String], stdout: &mut O) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("provider", stdout)?;
        return Ok(());
    }

    if !args.is_empty() {
        return Err(CliError::usage("usage: devenv provider list".to_owned()));
    }

    let registry = builtin_provider_registry();
    writeln!(stdout, "Providers")?;
    for capability in registry.providers() {
        write_provider_summary(capability, stdout)?;
    }
    Ok(())
}

fn run_provider_info_command<O>(args: &[String], stdout: &mut O) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("provider", stdout)?;
        return Ok(());
    }

    if args.is_empty() || args.len() > 2 {
        return Err(CliError::usage(
            "usage: devenv provider info <tool> [provider]".to_owned(),
        ));
    }

    let registry = builtin_provider_registry();
    let tool = ToolName::new(&args[0]).map_err(CoreError::from)?;
    let providers = match args.get(1) {
        Some(provider_name) => {
            let provider = ProviderId::new(provider_name).map_err(CoreError::from)?;
            let Some(capability) = registry.find(&tool, &provider) else {
                return Err(unknown_provider_id_error(
                    &tool,
                    &provider,
                    registry.providers_for_tool(&tool),
                ));
            };
            vec![capability]
        }
        None => registry.providers_for_tool(&tool),
    };
    if providers.is_empty() {
        return Err(unknown_provider_tool_error(&tool, registry.providers()));
    }

    writeln!(stdout, "Provider info for {tool}")?;
    for capability in providers {
        write_provider_detail(capability, stdout)?;
    }
    Ok(())
}

fn write_metadata_status<O>(
    capability: &ProviderCapability,
    cache: &FileMetadataCache,
    clock: &dyn Clock,
    stdout: &mut O,
) -> Result<(), CliError>
where
    O: Write,
{
    let key = metadata_cache_key(capability);
    let status = cache.metadata_status(&key, clock)?;
    let metadata_source = metadata_status_source_label(&status);
    write!(
        stdout,
        "{} {} support={} source={} checksum={} selectors={} cache={} metadata_source={}",
        capability.tool(),
        capability.provider(),
        support_level_label(capability.support_level()),
        capability.source_kind().as_str(),
        capability.checksum_policy().as_str(),
        selector_list(capability),
        status.as_str(),
        metadata_source
    )?;
    match &status {
        MetadataCacheStatus::Fresh(entry) | MetadataCacheStatus::Stale(entry) => {
            write_metadata_cache_source_fields(entry, stdout)?;
        }
        MetadataCacheStatus::Corrupt { reason } => {
            write!(stdout, " reason={reason}")?;
        }
        MetadataCacheStatus::Missing => {}
    }
    if let Some(next_action) = capability.next_action() {
        write!(stdout, " next=\"{}\"", escape_quoted_field(next_action))?;
    }
    writeln!(stdout)?;
    Ok(())
}

fn metadata_status_source_label(status: &MetadataCacheStatus) -> String {
    match status {
        MetadataCacheStatus::Fresh(entry) => metadata_cache_source_label(entry),
        MetadataCacheStatus::Stale(_) => "stale-cache".to_owned(),
        MetadataCacheStatus::Corrupt { .. } => "cache".to_owned(),
        MetadataCacheStatus::Missing => "missing".to_owned(),
    }
}

fn metadata_cache_source_label(entry: &MetadataCacheEntry) -> String {
    match entry
        .validator_metadata()
        .get("source_kind")
        .map(String::as_str)
    {
        Some("catalog") => "catalog".to_owned(),
        Some("env-fixture") => "env".to_owned(),
        Some("official" | "official-http" | "official-fixture") => "official".to_owned(),
        Some(_) | None => "cache".to_owned(),
    }
}

fn write_metadata_cache_source_fields<O>(
    entry: &MetadataCacheEntry,
    stdout: &mut O,
) -> Result<(), CliError>
where
    O: Write,
{
    let metadata = entry.validator_metadata();
    if let Some(source_kind) = metadata.get("source_kind") {
        write!(stdout, " cache_source={source_kind}")?;
    }
    for key in ["catalog_version", "manifest_sha256", "payload_sha256"] {
        if let Some(value) = metadata.get(key) {
            write!(stdout, " {key}={value}")?;
        }
    }
    Ok(())
}

fn write_provider_summary<O>(
    capability: &ProviderCapability,
    stdout: &mut O,
) -> Result<(), CliError>
where
    O: Write,
{
    writeln!(
        stdout,
        "{} {} support={} source={} checksum={} selectors={} platforms={}",
        capability.tool(),
        capability.provider(),
        support_level_label(capability.support_level()),
        capability.source_kind().as_str(),
        capability.checksum_policy().as_str(),
        selector_list(capability),
        platform_list(capability)
    )?;
    Ok(())
}

fn write_provider_detail<O>(capability: &ProviderCapability, stdout: &mut O) -> Result<(), CliError>
where
    O: Write,
{
    writeln!(stdout, "provider {}", capability.provider())?;
    writeln!(stdout, "display_name {}", capability.display_name())?;
    writeln!(
        stdout,
        "support {}",
        support_level_label(capability.support_level())
    )?;
    writeln!(stdout, "source {}", capability.source_kind().as_str())?;
    write_provider_catalog_availability(capability, stdout)?;
    writeln!(stdout, "checksum {}", capability.checksum_policy().as_str())?;
    writeln!(stdout, "selectors {}", selector_list(capability))?;
    writeln!(stdout, "platforms {}", platform_list(capability))?;
    if let Some(reason) = capability.direct_install_unavailable_reason() {
        writeln!(stdout, "direct_install_unavailable {reason}")?;
    }
    if let Some(next_action) = capability.next_action() {
        writeln!(stdout, "next_action {next_action}")?;
    }
    Ok(())
}

fn write_provider_catalog_availability<O>(
    capability: &ProviderCapability,
    stdout: &mut O,
) -> Result<(), CliError>
where
    O: Write,
{
    if catalog_provider_supported(capability) {
        writeln!(stdout, "catalog_availability available")?;
        writeln!(stdout, "catalog_status experimental")?;
        writeln!(stdout, "catalog_source_env {DEVENV_CATALOG_BASE_URL_ENV}")?;
        writeln!(stdout, "catalog_enable_env {DEVENV_ENABLE_CATALOG_ENV}")?;
        return Ok(());
    }

    let reason = if capability.support_level() != SupportLevel::Direct {
        capability
            .direct_install_unavailable_reason()
            .unwrap_or_else(|| "catalog metadata is not applicable to this provider".to_owned())
    } else {
        "catalog metadata path is not implemented for this provider in Phase 003".to_owned()
    };
    writeln!(stdout, "catalog_availability unavailable")?;
    writeln!(stdout, "catalog_unavailable_reason {reason}")?;
    Ok(())
}

fn catalog_provider_supported(capability: &ProviderCapability) -> bool {
    matches!(
        (capability.tool().as_str(), capability.provider().as_str()),
        ("go", "official") | ("node", "official") | ("terraform", "hashicorp")
    )
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MetadataUpdateOutcome {
    Updated,
    Skipped(String),
}

#[allow(dead_code)]
mod metadata_source {
    use super::{
        BTreeMap, CatalogTrustFailure, CliError, CommandContext, CoreError,
        DEVENV_ENABLE_CATALOG_ENV,
    };
    use std::str::FromStr;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub(crate) enum MetadataSourceMode {
        #[default]
        Auto,
        Env,
        Cache,
        Catalog,
        Official,
    }

    impl MetadataSourceMode {
        pub(crate) fn as_str(self) -> &'static str {
            match self {
                Self::Auto => "auto",
                Self::Env => "env",
                Self::Cache => "cache",
                Self::Catalog => "catalog",
                Self::Official => "official",
            }
        }

        pub(crate) fn requires_network(self) -> bool {
            matches!(self, Self::Catalog | Self::Official)
        }
    }

    impl FromStr for MetadataSourceMode {
        type Err = MetadataSourceModeParseError;

        fn from_str(value: &str) -> Result<Self, Self::Err> {
            match value {
                "auto" => Ok(Self::Auto),
                "env" => Ok(Self::Env),
                "cache" => Ok(Self::Cache),
                "catalog" => Ok(Self::Catalog),
                "official" => Ok(Self::Official),
                _ => Err(MetadataSourceModeParseError {
                    value: value.to_owned(),
                }),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct MetadataSourceModeParseError {
        value: String,
    }

    impl std::fmt::Display for MetadataSourceModeParseError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                formatter,
                "unknown metadata source `{}`; expected one of: auto, env, cache, catalog, official",
                self.value
            )
        }
    }

    impl std::error::Error for MetadataSourceModeParseError {}

    pub(crate) fn parse_metadata_source_mode(value: &str) -> Result<MetadataSourceMode, CliError> {
        value
            .parse::<MetadataSourceMode>()
            .map_err(|error| CliError::usage(error.to_string()))
    }

    pub(crate) fn metadata_catalog_gate_enabled(value: Option<&str>) -> bool {
        match value.map(str::trim) {
            Some("1") => true,
            Some(value) if value.eq_ignore_ascii_case("true") => true,
            Some(value) if value.eq_ignore_ascii_case("yes") => true,
            Some(value) if value.eq_ignore_ascii_case("on") => true,
            _ => false,
        }
    }

    pub(crate) fn context_catalog_gate_enabled(context: &CommandContext) -> bool {
        metadata_catalog_gate_enabled(
            context
                .env_vars
                .get(DEVENV_ENABLE_CATALOG_ENV)
                .map(String::as_str),
        )
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) enum SelectedMetadataSourceKind {
        EnvFixture,
        Cache,
        Catalog,
        Official,
        StaleCache,
    }

    impl SelectedMetadataSourceKind {
        pub(crate) fn as_str(self) -> &'static str {
            match self {
                Self::EnvFixture => "env-fixture",
                Self::Cache => "cache",
                Self::Catalog => "catalog",
                Self::Official => "official",
                Self::StaleCache => "stale-cache",
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct SelectedMetadataSource {
        kind: SelectedMetadataSourceKind,
        metadata: BTreeMap<String, String>,
    }

    impl SelectedMetadataSource {
        pub(crate) fn new(kind: SelectedMetadataSourceKind) -> Self {
            Self {
                kind,
                metadata: BTreeMap::new(),
            }
        }

        pub(crate) fn env_fixture() -> Self {
            Self::new(SelectedMetadataSourceKind::EnvFixture)
        }

        pub(crate) fn cache() -> Self {
            Self::new(SelectedMetadataSourceKind::Cache)
        }

        pub(crate) fn catalog() -> Self {
            Self::new(SelectedMetadataSourceKind::Catalog)
        }

        pub(crate) fn official() -> Self {
            Self::new(SelectedMetadataSourceKind::Official)
        }

        pub(crate) fn stale_cache() -> Self {
            Self::new(SelectedMetadataSourceKind::StaleCache)
        }

        pub(crate) fn kind(&self) -> SelectedMetadataSourceKind {
            self.kind
        }

        pub(crate) fn with_metadata(
            mut self,
            key: impl Into<String>,
            value: impl Into<String>,
        ) -> Self {
            self.metadata.insert(key.into(), value.into());
            self
        }

        pub(crate) fn with_catalog_version(self, value: impl Into<String>) -> Self {
            self.with_metadata("catalog_version", value)
        }

        pub(crate) fn with_manifest_sha256(self, value: impl Into<String>) -> Self {
            self.with_metadata("manifest_sha256", value)
        }

        pub(crate) fn with_payload_sha256(self, value: impl Into<String>) -> Self {
            self.with_metadata("payload_sha256", value)
        }

        pub(crate) fn validator_metadata(&self) -> BTreeMap<String, String> {
            let mut metadata = self.metadata.clone();
            metadata.insert("source_kind".to_owned(), self.kind.as_str().to_owned());
            metadata
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) struct MetadataSourceResolutionRequest {
        mode: MetadataSourceMode,
        offline: bool,
        catalog_enabled: bool,
    }

    impl MetadataSourceResolutionRequest {
        pub(crate) fn new(mode: MetadataSourceMode) -> Self {
            Self {
                mode,
                offline: false,
                catalog_enabled: false,
            }
        }

        pub(crate) fn from_context(
            mode: MetadataSourceMode,
            offline: bool,
            context: &CommandContext,
        ) -> Self {
            Self {
                mode,
                offline,
                catalog_enabled: context_catalog_gate_enabled(context),
            }
        }

        pub(crate) fn offline(mut self, offline: bool) -> Self {
            self.offline = offline;
            self
        }

        pub(crate) fn catalog_enabled(mut self, catalog_enabled: bool) -> Self {
            self.catalog_enabled = catalog_enabled;
            self
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub(crate) enum MetadataSourceCandidate {
        Available(SelectedMetadataSource),
        #[default]
        Missing,
        NetworkFailure(String),
        TrustFailure(CatalogTrustFailure),
    }

    impl MetadataSourceCandidate {
        pub(crate) fn available(source: SelectedMetadataSource) -> Self {
            Self::Available(source)
        }

        pub(crate) fn network_failure(message: impl Into<String>) -> Self {
            Self::NetworkFailure(message.into())
        }

        pub(crate) fn trust_failure(failure: CatalogTrustFailure) -> Self {
            Self::TrustFailure(failure)
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Default)]
    pub(crate) struct MetadataSourceCandidates {
        pub(crate) env_override: Option<SelectedMetadataSource>,
        pub(crate) cache: Option<SelectedMetadataSource>,
        pub(crate) catalog: MetadataSourceCandidate,
        pub(crate) official: MetadataSourceCandidate,
        pub(crate) stale_cache: Option<SelectedMetadataSource>,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub(crate) enum MetadataSourceResolutionError {
        Validation(String),
        Unavailable(String),
        Network(String),
        Trust(CatalogTrustFailure),
    }

    impl std::fmt::Display for MetadataSourceResolutionError {
        fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::Validation(message) | Self::Unavailable(message) | Self::Network(message) => {
                    formatter.write_str(message)
                }
                Self::Trust(failure) => write!(formatter, "catalog trust failure: {failure}"),
            }
        }
    }

    impl std::error::Error for MetadataSourceResolutionError {}

    impl From<MetadataSourceResolutionError> for CliError {
        fn from(value: MetadataSourceResolutionError) -> Self {
            match value {
                MetadataSourceResolutionError::Validation(message) => CliError::usage(message),
                MetadataSourceResolutionError::Unavailable(message) => CliError::runtime(message),
                MetadataSourceResolutionError::Network(message) => {
                    CliError::from(CoreError::catalog_network(message))
                }
                MetadataSourceResolutionError::Trust(failure) => {
                    CliError::from(CoreError::catalog_trust(failure))
                }
            }
        }
    }

    pub(crate) fn validate_metadata_source_request(
        request: &MetadataSourceResolutionRequest,
    ) -> Result<(), MetadataSourceResolutionError> {
        if request.offline && request.mode.requires_network() {
            return Err(MetadataSourceResolutionError::Validation(format!(
                "`--offline` cannot be combined with `--source {}` because that source requires network access",
                request.mode.as_str()
            )));
        }
        Ok(())
    }

    pub(crate) fn resolve_metadata_source_selection(
        request: MetadataSourceResolutionRequest,
        candidates: MetadataSourceCandidates,
    ) -> Result<SelectedMetadataSource, MetadataSourceResolutionError> {
        validate_metadata_source_request(&request)?;

        if let Some(source) = candidates.env_override {
            return Ok(source);
        }

        match request.mode {
            MetadataSourceMode::Env => Err(MetadataSourceResolutionError::Unavailable(
                "metadata env fixture source is not configured".to_owned(),
            )),
            MetadataSourceMode::Cache => candidates.cache.ok_or_else(|| {
                MetadataSourceResolutionError::Unavailable(
                    "metadata cache source is missing or stale".to_owned(),
                )
            }),
            MetadataSourceMode::Catalog => {
                resolve_explicit_metadata_source("catalog", candidates.catalog)
            }
            MetadataSourceMode::Official => {
                resolve_explicit_metadata_source("official", candidates.official)
            }
            MetadataSourceMode::Auto => resolve_auto_metadata_source(request, candidates),
        }
    }

    fn resolve_explicit_metadata_source(
        source_name: &str,
        candidate: MetadataSourceCandidate,
    ) -> Result<SelectedMetadataSource, MetadataSourceResolutionError> {
        match candidate {
            MetadataSourceCandidate::Available(source) => Ok(source),
            MetadataSourceCandidate::Missing => Err(MetadataSourceResolutionError::Unavailable(
                format!("{source_name} metadata source is not available"),
            )),
            MetadataSourceCandidate::NetworkFailure(message) => Err(
                MetadataSourceResolutionError::Network(format!("{source_name}: {message}")),
            ),
            MetadataSourceCandidate::TrustFailure(failure) => {
                Err(MetadataSourceResolutionError::Trust(failure))
            }
        }
    }

    fn resolve_auto_metadata_source(
        request: MetadataSourceResolutionRequest,
        candidates: MetadataSourceCandidates,
    ) -> Result<SelectedMetadataSource, MetadataSourceResolutionError> {
        if let Some(source) = candidates.cache {
            return Ok(source);
        }

        if request.offline {
            return candidates.stale_cache.ok_or_else(|| {
            MetadataSourceResolutionError::Unavailable(
                "metadata cache source is missing and offline mode disables catalog and official sources"
                    .to_owned(),
            )
        });
        }

        let mut network_failure = None;
        if request.catalog_enabled {
            match candidates.catalog {
                MetadataSourceCandidate::Available(source) => return Ok(source),
                MetadataSourceCandidate::Missing => {}
                MetadataSourceCandidate::NetworkFailure(message) => {
                    network_failure.get_or_insert_with(|| format!("catalog: {message}"));
                }
                MetadataSourceCandidate::TrustFailure(failure) => {
                    return Err(MetadataSourceResolutionError::Trust(failure));
                }
            }
        }

        match candidates.official {
            MetadataSourceCandidate::Available(source) => return Ok(source),
            MetadataSourceCandidate::Missing => {}
            MetadataSourceCandidate::NetworkFailure(message) => {
                network_failure.get_or_insert_with(|| format!("official: {message}"));
            }
            MetadataSourceCandidate::TrustFailure(failure) => {
                return Err(MetadataSourceResolutionError::Trust(failure));
            }
        }

        if let Some(source) = candidates.stale_cache {
            return Ok(source);
        }

        if let Some(message) = network_failure {
            Err(MetadataSourceResolutionError::Network(message))
        } else {
            Err(MetadataSourceResolutionError::Unavailable(
                "no metadata source is available".to_owned(),
            ))
        }
    }
}

use metadata_source::{MetadataSourceMode, SelectedMetadataSource, parse_metadata_source_mode};

fn metadata_update_skipped_reason(capability: &ProviderCapability, reason: &str) -> String {
    let mut message = format!(
        "support={} reason=\"{}\"",
        support_level_label(capability.support_level()),
        escape_quoted_field(reason)
    );
    if let Some(next_action) = capability.next_action() {
        message.push_str(&format!(" next=\"{}\"", escape_quoted_field(next_action)));
    }
    message
}

fn metadata_update_skipped_error(capability: &ProviderCapability, reason: &str) -> CliError {
    let next = capability
        .next_action()
        .map(|value| format!("\nnext: {value}"))
        .unwrap_or_default();
    CliError::runtime(format!(
        "metadata update is not supported for {}: provider {} is {}. {reason}{next}",
        capability.tool(),
        capability.provider(),
        support_level_label(capability.support_level())
    ))
}

fn update_metadata_provider(
    capability: &ProviderCapability,
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
    allow_live_fetch: bool,
    source_mode: MetadataSourceMode,
) -> Result<MetadataUpdateOutcome, CliError> {
    if capability.support_level() != SupportLevel::Direct {
        return Ok(MetadataUpdateOutcome::Skipped(
            capability
                .direct_install_unavailable_reason()
                .unwrap_or_else(|| "remote metadata refresh is not supported".to_owned()),
        ));
    }

    let Some(input) = release_metadata_input_for_capability(capability) else {
        return Ok(MetadataUpdateOutcome::Skipped(
            "no metadata source is configured for this provider".to_owned(),
        ));
    };

    if !context.env_vars.contains_key(input.env_key) {
        if capability.tool().as_str() == "go" && capability.provider().as_str() == "official" {
            return update_go_metadata_provider(
                cache,
                clock,
                context,
                allow_live_fetch,
                source_mode,
            );
        }
        if capability.tool().as_str() == "node" && capability.provider().as_str() == "official" {
            return update_node_metadata_provider(
                cache,
                clock,
                context,
                allow_live_fetch,
                source_mode,
            );
        }
        if capability.tool().as_str() == "terraform"
            && capability.provider().as_str() == "hashicorp"
        {
            return update_iac_metadata_provider(
                IacTool::Terraform,
                cache,
                clock,
                context,
                allow_live_fetch,
                source_mode,
            );
        }
        if capability.tool().as_str() == "java" && capability.provider().as_str() == "temurin" {
            return update_java_temurin_metadata_provider(
                cache,
                clock,
                context,
                allow_live_fetch,
                source_mode,
            );
        }
        if capability.tool().as_str() == "flutter"
            && capability.provider().as_str() == "stable"
            && (context
                .env_vars
                .contains_key(FLUTTER_OFFICIAL_RELEASES_DIR_ENV)
                || context.env_vars.contains_key(FLUTTER_OFFICIAL_BASE_URL_ENV)
                || allow_live_fetch)
        {
            refresh_flutter_stable_metadata(cache, clock, context)?;
            return Ok(MetadataUpdateOutcome::Updated);
        }
        if capability.tool().as_str() == "opentofu"
            && capability.provider().as_str() == "opentofu"
            && (context
                .env_vars
                .contains_key(OPENTOFU_OFFICIAL_RELEASES_ENV)
                || context
                    .env_vars
                    .contains_key(OPENTOFU_OFFICIAL_BASE_URL_ENV)
                || allow_live_fetch)
        {
            refresh_iac_official_metadata(IacTool::OpenTofu, cache, clock, context)?;
            return Ok(MetadataUpdateOutcome::Updated);
        }
        return Ok(MetadataUpdateOutcome::Skipped(format!(
            "missing {}; live HTTP refresh is not implemented yet",
            input.env_key
        )));
    }

    let loaded = load_release_metadata_payload(context, input)?;
    validate_release_metadata_for_capability(capability, &loaded.contents)?;

    let payload_sha256 = format!("sha256:{}", hex_sha256(loaded.contents.as_bytes()));
    let selected_source = SelectedMetadataSource::env_fixture()
        .with_metadata("source_env", input.env_key)
        .with_metadata("provider", capability.provider().as_str());
    let mut entry = MetadataCacheEntry::new(
        metadata_cache_key(capability),
        loaded.source_url,
        clock.now_utc()?,
        METADATA_CACHE_TTL_SECONDS,
        payload_sha256,
        MetadataPayloadKind::Raw,
        loaded.contents,
    );
    for (key, value) in selected_source.validator_metadata() {
        entry = entry.with_validator_metadata(key, value);
    }

    cache.write_metadata(entry)?;
    Ok(MetadataUpdateOutcome::Updated)
}

fn update_java_temurin_metadata_provider(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
    allow_live_fetch: bool,
    source_mode: MetadataSourceMode,
) -> Result<MetadataUpdateOutcome, CliError> {
    match source_mode {
        MetadataSourceMode::Env => Ok(MetadataUpdateOutcome::Skipped(format!(
            "missing {}; env source was explicitly requested",
            JAVA_RELEASE_METADATA_ENV
        ))),
        MetadataSourceMode::Cache => Ok(MetadataUpdateOutcome::Skipped(
            "cache source cannot refresh provider metadata".to_owned(),
        )),
        MetadataSourceMode::Catalog => Ok(MetadataUpdateOutcome::Skipped(
            "catalog source is not implemented for java/temurin yet".to_owned(),
        )),
        MetadataSourceMode::Official => {
            if context
                .env_vars
                .contains_key(JAVA_TEMURIN_RELEASE_METADATA_ENV)
            {
                refresh_java_temurin_metadata_from_fixture(cache, clock, context)?;
                Ok(MetadataUpdateOutcome::Updated)
            } else if allow_live_fetch {
                refresh_java_temurin_metadata_from_provider_manifest(cache, clock, context)?;
                Ok(MetadataUpdateOutcome::Updated)
            } else {
                Ok(MetadataUpdateOutcome::Skipped(format!(
                    "missing {}; live HTTP refresh is not enabled in this context",
                    JAVA_TEMURIN_RELEASE_METADATA_ENV
                )))
            }
        }
        MetadataSourceMode::Auto => {
            if context
                .env_vars
                .contains_key(JAVA_TEMURIN_RELEASE_METADATA_ENV)
            {
                refresh_java_temurin_metadata_from_fixture(cache, clock, context)?;
                return Ok(MetadataUpdateOutcome::Updated);
            }
            if allow_live_fetch {
                refresh_java_temurin_metadata_from_provider_manifest(cache, clock, context)?;
                return Ok(MetadataUpdateOutcome::Updated);
            }
            Ok(MetadataUpdateOutcome::Skipped(format!(
                "missing {}; live HTTP refresh is not enabled in this context",
                JAVA_TEMURIN_RELEASE_METADATA_ENV
            )))
        }
    }
}

fn update_go_metadata_provider(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
    allow_live_fetch: bool,
    source_mode: MetadataSourceMode,
) -> Result<MetadataUpdateOutcome, CliError> {
    match source_mode {
        MetadataSourceMode::Env => Ok(MetadataUpdateOutcome::Skipped(format!(
            "missing {}; env source was explicitly requested",
            GO_RELEASE_METADATA_ENV
        ))),
        MetadataSourceMode::Cache => Ok(MetadataUpdateOutcome::Skipped(
            "cache source cannot refresh provider metadata".to_owned(),
        )),
        MetadataSourceMode::Catalog => {
            refresh_go_catalog_metadata(cache, clock, context)?;
            Ok(MetadataUpdateOutcome::Updated)
        }
        MetadataSourceMode::Official => {
            if context
                .env_vars
                .contains_key(GO_OFFICIAL_RELEASE_METADATA_ENV)
                || allow_live_fetch
            {
                refresh_go_official_metadata(cache, clock, context)?;
                Ok(MetadataUpdateOutcome::Updated)
            } else {
                Ok(MetadataUpdateOutcome::Skipped(format!(
                    "missing {}; live HTTP refresh is not enabled in this context",
                    GO_RELEASE_METADATA_ENV
                )))
            }
        }
        MetadataSourceMode::Auto => {
            if context_catalog_update_enabled(context) {
                refresh_go_catalog_metadata(cache, clock, context)?;
                return Ok(MetadataUpdateOutcome::Updated);
            }
            if context
                .env_vars
                .contains_key(GO_OFFICIAL_RELEASE_METADATA_ENV)
                || allow_live_fetch
            {
                refresh_go_official_metadata(cache, clock, context)?;
                return Ok(MetadataUpdateOutcome::Updated);
            }
            Ok(MetadataUpdateOutcome::Skipped(format!(
                "missing {}; live HTTP refresh is not implemented yet",
                GO_RELEASE_METADATA_ENV
            )))
        }
    }
}

fn update_node_metadata_provider(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
    allow_live_fetch: bool,
    source_mode: MetadataSourceMode,
) -> Result<MetadataUpdateOutcome, CliError> {
    match source_mode {
        MetadataSourceMode::Env => Ok(MetadataUpdateOutcome::Skipped(format!(
            "missing {}; env source was explicitly requested",
            NODE_RELEASE_METADATA_ENV
        ))),
        MetadataSourceMode::Cache => Ok(MetadataUpdateOutcome::Skipped(
            "cache source cannot refresh provider metadata".to_owned(),
        )),
        MetadataSourceMode::Catalog => {
            refresh_node_catalog_metadata(cache, clock, context)?;
            Ok(MetadataUpdateOutcome::Updated)
        }
        MetadataSourceMode::Official => {
            if context
                .env_vars
                .contains_key(NODE_OFFICIAL_RELEASE_INDEX_ENV)
                || context.env_vars.contains_key(NODE_OFFICIAL_BASE_URL_ENV)
                || allow_live_fetch
            {
                refresh_node_official_metadata(cache, clock, context)?;
                Ok(MetadataUpdateOutcome::Updated)
            } else {
                Ok(MetadataUpdateOutcome::Skipped(format!(
                    "missing {}; live HTTP refresh is not enabled in this context",
                    NODE_RELEASE_METADATA_ENV
                )))
            }
        }
        MetadataSourceMode::Auto => {
            if context_catalog_update_enabled(context) {
                refresh_node_catalog_metadata(cache, clock, context)?;
                return Ok(MetadataUpdateOutcome::Updated);
            }
            if context
                .env_vars
                .contains_key(NODE_OFFICIAL_RELEASE_INDEX_ENV)
                || context.env_vars.contains_key(NODE_OFFICIAL_BASE_URL_ENV)
                || allow_live_fetch
            {
                refresh_node_official_metadata(cache, clock, context)?;
                return Ok(MetadataUpdateOutcome::Updated);
            }
            Ok(MetadataUpdateOutcome::Skipped(format!(
                "missing {}; live HTTP refresh is not implemented yet",
                NODE_RELEASE_METADATA_ENV
            )))
        }
    }
}

fn update_iac_metadata_provider(
    tool: IacTool,
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
    allow_live_fetch: bool,
    source_mode: MetadataSourceMode,
) -> Result<MetadataUpdateOutcome, CliError> {
    match source_mode {
        MetadataSourceMode::Env => Ok(MetadataUpdateOutcome::Skipped(format!(
            "missing {}; env source was explicitly requested",
            iac_release_metadata_env(tool)
        ))),
        MetadataSourceMode::Cache => Ok(MetadataUpdateOutcome::Skipped(
            "cache source cannot refresh provider metadata".to_owned(),
        )),
        MetadataSourceMode::Catalog => {
            refresh_iac_catalog_metadata(tool, cache, clock, context)?;
            Ok(MetadataUpdateOutcome::Updated)
        }
        MetadataSourceMode::Official => {
            if context.env_vars.contains_key(iac_official_index_env(tool))
                || context
                    .env_vars
                    .contains_key(iac_official_base_url_env(tool))
                || allow_live_fetch
            {
                refresh_iac_official_metadata(tool, cache, clock, context)?;
                Ok(MetadataUpdateOutcome::Updated)
            } else {
                Ok(MetadataUpdateOutcome::Skipped(format!(
                    "missing {}; live HTTP refresh is not enabled in this context",
                    iac_release_metadata_env(tool)
                )))
            }
        }
        MetadataSourceMode::Auto => {
            if context_catalog_update_enabled(context) {
                refresh_iac_catalog_metadata(tool, cache, clock, context)?;
                return Ok(MetadataUpdateOutcome::Updated);
            }
            if context.env_vars.contains_key(iac_official_index_env(tool))
                || context
                    .env_vars
                    .contains_key(iac_official_base_url_env(tool))
                || allow_live_fetch
            {
                refresh_iac_official_metadata(tool, cache, clock, context)?;
                return Ok(MetadataUpdateOutcome::Updated);
            }
            Ok(MetadataUpdateOutcome::Skipped(format!(
                "missing {}; live HTTP refresh is not implemented yet",
                iac_release_metadata_env(tool)
            )))
        }
    }
}

fn context_catalog_update_enabled(context: &CommandContext) -> bool {
    metadata_source::context_catalog_gate_enabled(context)
        && context.env_vars.contains_key(DEVENV_CATALOG_BASE_URL_ENV)
}

fn validate_release_metadata_for_capability(
    capability: &ProviderCapability,
    contents: &str,
) -> Result<(), CliError> {
    match (capability.tool().as_str(), capability.provider().as_str()) {
        ("go", "official") => GoReleaseMetadata::parse(contents).map(|_| ()),
        ("java", "temurin") => JavaTemurinReleaseMetadata::parse(contents)
            .and_then(JavaTemurinReleaseMetadata::into_release_metadata)
            .map(|_| ())
            .or_else(|_| JavaReleaseMetadata::parse(contents).map(|_| ())),
        ("node", "official") => NodeReleaseMetadata::parse(contents)
            .map(|_| ())
            .or_else(|_| {
                let (index_json, shasums_by_version) =
                    decode_node_official_metadata_bundle(contents)
                        .map_err(|error| CoreError::message(error.to_string()))?;
                NodeOfficialReleaseMetadata::parse(&index_json, &shasums_by_version)
                    .and_then(NodeOfficialReleaseMetadata::into_release_metadata)
                    .map(|_| ())
            }),
        ("python", "cpython") => PythonReleaseMetadata::parse(contents).map(|_| ()),
        ("flutter", "stable") => {
            FlutterReleaseMetadata::parse(contents)
                .map(|_| ())
                .or_else(|_| {
                    let payloads = decode_flutter_official_metadata_bundle(contents)
                        .map_err(|error| CoreError::message(error.to_string()))?;
                    FlutterOfficialReleaseMetadata::parse_stable(&payloads)
                        .and_then(FlutterOfficialReleaseMetadata::into_release_metadata)
                        .map(|_| ())
                })
        }
        ("terraform", "hashicorp") => {
            IacReleaseMetadata::parse(contents)
                .map(|_| ())
                .or_else(|_| {
                    let (payload, checksums_by_version) =
                        decode_iac_official_metadata_bundle(IacTool::Terraform, contents)
                            .map_err(|error| CoreError::message(error.to_string()))?;
                    IacOfficialReleaseMetadata::parse_terraform(&payload, &checksums_by_version)
                        .and_then(IacOfficialReleaseMetadata::into_release_metadata)
                        .map(|_| ())
                })
        }
        ("opentofu", "opentofu") => IacReleaseMetadata::parse(contents)
            .map(|_| ())
            .or_else(|_| {
                let (payload, checksums_by_version) =
                    decode_iac_official_metadata_bundle(IacTool::OpenTofu, contents)
                        .map_err(|error| CoreError::message(error.to_string()))?;
                IacOfficialReleaseMetadata::parse_opentofu(&payload, &checksums_by_version)
                    .and_then(IacOfficialReleaseMetadata::into_release_metadata)
                    .map(|_| ())
            }),
        _ => Err(CoreError::message(format!(
            "no metadata parser is registered for {} {}",
            capability.tool(),
            capability.provider()
        ))),
    }
    .map_err(CliError::from)
}

fn metadata_cache_key(capability: &ProviderCapability) -> MetadataCacheKey {
    MetadataCacheKey::new(capability.tool().clone(), capability.provider().clone())
}

fn release_metadata_input_for_capability(
    capability: &ProviderCapability,
) -> Option<ReleaseMetadataInput> {
    match (capability.tool().as_str(), capability.provider().as_str()) {
        ("go", "official") => Some(ReleaseMetadataInput::fixture("Go", GO_RELEASE_METADATA_ENV)),
        ("java", "temurin") => Some(ReleaseMetadataInput::fixture(
            "Java",
            JAVA_RELEASE_METADATA_ENV,
        )),
        ("node", "official") => Some(ReleaseMetadataInput::fixture(
            "Node.js",
            NODE_RELEASE_METADATA_ENV,
        )),
        ("python", "cpython") => Some(ReleaseMetadataInput::fixture(
            "Python",
            PYTHON_RELEASE_METADATA_ENV,
        )),
        ("flutter", "stable") => Some(ReleaseMetadataInput::fixture(
            "Flutter",
            FLUTTER_RELEASE_METADATA_ENV,
        )),
        ("terraform", "hashicorp") => Some(ReleaseMetadataInput::fixture(
            "Terraform",
            TERRAFORM_RELEASE_METADATA_ENV,
        )),
        ("opentofu", "opentofu") => Some(ReleaseMetadataInput::fixture(
            "OpenTofu",
            OPENTOFU_RELEASE_METADATA_ENV,
        )),
        _ => None,
    }
}

fn support_level_label(level: SupportLevel) -> &'static str {
    match level {
        SupportLevel::Direct => "Direct",
        SupportLevel::Delegated => "Delegated",
        SupportLevel::LocalOnly => "LocalOnly",
    }
}

fn selector_list(capability: &ProviderCapability) -> String {
    let selectors = capability
        .selector_dimensions()
        .iter()
        .map(|selector| selector.as_str())
        .collect::<Vec<_>>();
    if selectors.is_empty() {
        "-".to_owned()
    } else {
        selectors.join(",")
    }
}

fn platform_list(capability: &ProviderCapability) -> String {
    let platforms = capability
        .platform_support()
        .platforms()
        .iter()
        .map(Platform::id)
        .collect::<Vec<_>>();
    if platforms.is_empty() {
        "-".to_owned()
    } else {
        platforms.join(",")
    }
}

fn unknown_provider_tool_error(tool: &ToolName, capabilities: &[ProviderCapability]) -> CliError {
    let supported = capabilities
        .iter()
        .map(|capability| capability.tool().as_str().to_owned())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ");

    CliError::runtime(format!(
        "unknown tool `{tool}` for provider metadata\nsupported provider tools: {supported}\nnext: run `devenv provider list`"
    ))
}

fn unknown_provider_id_error(
    tool: &ToolName,
    provider: &ProviderId,
    capabilities: Vec<&ProviderCapability>,
) -> CliError {
    if capabilities.is_empty() {
        return unknown_provider_tool_error(tool, builtin_provider_registry().providers());
    }

    let supported = capabilities
        .iter()
        .map(|capability| capability.provider().as_str().to_owned())
        .collect::<Vec<_>>()
        .join(", ");

    CliError::runtime(format!(
        "unknown provider `{provider}` for {tool}\nsupported providers: {supported}\nnext: run `devenv provider info {tool}`"
    ))
}

fn catalog_unavailable_error(message: impl Into<String>) -> CliError {
    CliError::runtime(format!(
        "catalog unavailable: {}\nnext: set `{DEVENV_CATALOG_BASE_URL_ENV}`, pass `--catalog <path-or-url>`, or use `devenv metadata update <tool> --source official` when official provider fallback is acceptable.",
        message.into()
    ))
}

fn catalog_network_failure_error(message: impl Into<String>) -> CliError {
    CliError::runtime(format!(
        "catalog network failure: {}\nnext: verify `{DEVENV_CATALOG_BASE_URL_ENV}` or `--catalog <path-or-url>` points to a reachable immutable catalog release. In auto mode, retry with `--source official` if trust was not the failure.",
        message.into()
    ))
}

fn catalog_trust_failure_error(failure: CatalogTrustFailure) -> CliError {
    CliError::runtime(format!(
        "catalog trust failure: {failure}\nnext: do not ignore catalog trust failures. Verify manifest.sig, the configured trust root, catalog freshness, and payload sha256, or choose a newer signed catalog release."
    ))
}

fn unsupported_install_error(tool: &ToolName, requirement: &VersionRequirement) -> CliError {
    let registry = builtin_provider_registry();
    let providers = registry.providers_for_tool(tool);
    if providers.is_empty() {
        return CliError::runtime(format!(
            "`devenv install` is not implemented for `{tool}` yet\nnext: run `devenv provider list` to inspect supported tools"
        ));
    }

    if let Some(capability) = providers
        .iter()
        .find(|capability| capability.support_level() != SupportLevel::Direct)
    {
        let reason = capability
            .direct_install_unavailable_reason()
            .unwrap_or_else(|| "direct install is not available for this provider".to_owned());
        let next = capability
            .next_action()
            .unwrap_or("Use `devenv add <tool> <path>` to register an existing runtime.");
        return CliError::runtime(format!(
            "`devenv install {tool}@{}` is not supported by provider {} because support={}.\nreason: {reason}\nnext: {next}",
            requirement.raw(),
            capability.provider(),
            support_level_label(capability.support_level())
        ));
    }

    let providers = providers
        .iter()
        .map(|capability| capability.provider().as_str())
        .collect::<Vec<_>>()
        .join(", ");
    CliError::runtime(format!(
        "`devenv install` is not wired for `{tool}` yet even though provider metadata exists.\nproviders: {providers}\nnext: run `devenv provider info {tool}`"
    ))
}

fn unsupported_list_remote_error(tool: &ToolName) -> CliError {
    let registry = builtin_provider_registry();
    let providers = registry.providers_for_tool(tool);
    if providers.is_empty() {
        return CliError::runtime(format!(
            "`devenv list-remote` is not implemented for `{tool}` yet\nnext: run `devenv provider list` to inspect supported tools"
        ));
    }

    let capability = providers[0];
    let reason = capability
        .direct_install_unavailable_reason()
        .unwrap_or_else(|| "remote metadata listing is not available for this provider".to_owned());
    let next = capability
        .next_action()
        .unwrap_or("Use `devenv add <tool> <path>` to register an existing runtime.");
    CliError::runtime(format!(
        "`devenv list-remote {tool}` is not supported by provider {} because support={}.\nreason: {reason}\nnext: {next}",
        capability.provider(),
        support_level_label(capability.support_level())
    ))
}

fn escape_quoted_field(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn run_install_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("install", stdout)?;
        return Ok(());
    }

    let (spec_arg, options) = parse_install_args(args)?;
    let spec = parse_tool_spec(&spec_arg)?;
    if spec.tool().as_str() != "java" {
        reject_java_distribution_option_for_tool(spec.tool(), options.distribution.as_deref())?;
    }
    if spec.tool().as_str() != "flutter" {
        reject_flutter_channel_option_for_tool(spec.tool(), options.channel.as_deref())?;
    }
    match spec.tool().as_str() {
        "java" => run_install_java(spec.requirement(), options, stdout, context),
        "go" => run_install_go(spec.requirement(), options, stdout, context),
        "flutter" => run_install_flutter(spec.requirement(), options, stdout, context),
        "terraform" => run_install_iac(
            IacTool::Terraform,
            spec.requirement(),
            options,
            stdout,
            context,
        ),
        "opentofu" => run_install_iac(
            IacTool::OpenTofu,
            spec.requirement(),
            options,
            stdout,
            context,
        ),
        "node" => run_install_node(spec.requirement(), options, stdout, context),
        "python" => run_install_python(spec.requirement(), options, stdout, context),
        _ => Err(unsupported_install_error(spec.tool(), spec.requirement())),
    }
}

#[derive(Debug, Clone, Default)]
struct InstallOptions {
    dry_run: bool,
    distribution: Option<String>,
    channel: Option<String>,
}

fn parse_install_args(args: &[String]) -> Result<(String, InstallOptions), CliError> {
    if args.is_empty() {
        return Err(CliError::usage(INSTALL_USAGE.to_owned()));
    }

    let mut positionals = Vec::new();
    let mut options = InstallOptions::default();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        match arg.as_str() {
            "--dry-run" => {
                options.dry_run = true;
                index += 1;
            }
            "--distribution" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliError::usage(format!(
                        "`--distribution` requires a value\n{INSTALL_USAGE}"
                    )));
                };
                options.distribution = Some(value.clone());
                index += 2;
            }
            "--channel" => {
                let Some(value) = args.get(index + 1) else {
                    return Err(CliError::usage(format!(
                        "`--channel` requires a value\n{INSTALL_USAGE}"
                    )));
                };
                options.channel = Some(value.clone());
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::usage(format!(
                    "unknown install option `{value}`\n{INSTALL_USAGE}"
                )));
            }
            value => {
                positionals.push(value);
                index += 1;
            }
        }
    }

    let spec = normalize_install_target(&positionals, &mut options)?;

    Ok((spec, options))
}

fn normalize_install_target(
    positionals: &[&str],
    options: &mut InstallOptions,
) -> Result<String, CliError> {
    match positionals {
        [] => Err(CliError::usage(INSTALL_USAGE.to_owned())),
        [spec] if spec.contains('@') => Ok((*spec).to_owned()),
        [tool, version] if !tool.contains('@') => Ok(format!("{tool}@{version}")),
        [tool, version, selector] if !tool.contains('@') => {
            set_install_selector_from_positional(tool, selector, options)?;
            Ok(format!("{tool}@{version}"))
        }
        [spec, selector] if spec.contains('@') => {
            let Some((tool, _version)) = spec.split_once('@') else {
                return Err(CliError::usage(INSTALL_USAGE.to_owned()));
            };
            set_install_selector_from_positional(tool, selector, options)?;
            Ok((*spec).to_owned())
        }
        _ => Err(CliError::usage(INSTALL_USAGE.to_owned())),
    }
}

fn set_install_selector_from_positional(
    tool: &str,
    selector: &str,
    options: &mut InstallOptions,
) -> Result<(), CliError> {
    match tool.to_ascii_lowercase().as_str() {
        "java" => set_positional_option(
            "--distribution",
            &mut options.distribution,
            selector,
            "Java distribution",
        ),
        "flutter" => set_positional_option(
            "--channel",
            &mut options.channel,
            selector,
            "Flutter channel",
        ),
        _ => Err(CliError::usage(format!(
            "unexpected install selector `{selector}` for `{tool}`; positional selectors are supported for Java distributions and Flutter channels\n{INSTALL_USAGE}"
        ))),
    }
}

fn set_positional_option(
    flag: &str,
    option: &mut Option<String>,
    value: &str,
    label: &str,
) -> Result<(), CliError> {
    if let Some(existing) = option.as_deref() {
        if existing != value {
            return Err(CliError::usage(format!(
                "conflicting {label} values: positional `{value}` and {flag} `{existing}`"
            )));
        }
    } else {
        *option = Some(value.to_owned());
    }
    Ok(())
}

fn run_uninstall_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("uninstall", stdout)?;
        return Ok(());
    }

    let spec_arg = parse_uninstall_args(args)?;
    let spec = parse_tool_spec(&spec_arg)?;
    match spec.tool().as_str() {
        "java" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &JavaVersionMatcher,
            stdout,
            context,
        ),
        "go" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &GoVersionMatcher,
            stdout,
            context,
        ),
        "flutter" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &FlutterVersionMatcher,
            stdout,
            context,
        ),
        "terraform" | "opentofu" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &IacVersionMatcher,
            stdout,
            context,
        ),
        "node" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &NodeVersionMatcher,
            stdout,
            context,
        ),
        "python" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &PythonVersionMatcher,
            stdout,
            context,
        ),
        "ruby" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &RubyVersionMatcher,
            stdout,
            context,
        ),
        "php" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &PhpVersionMatcher,
            stdout,
            context,
        ),
        "rust" => run_uninstall_with_matcher(
            spec.tool(),
            spec.requirement(),
            &RustVersionMatcher,
            stdout,
            context,
        ),
        _ => Err(CliError::runtime(format!(
            "`devenv uninstall` is not implemented for `{}` yet",
            spec.tool()
        ))),
    }
}

fn parse_uninstall_args(args: &[String]) -> Result<String, CliError> {
    match args {
        [spec] if spec.contains('@') => Ok(spec.clone()),
        [tool, version] if !tool.contains('@') => Ok(format!("{tool}@{version}")),
        _ => Err(CliError::usage(UNINSTALL_USAGE.to_owned())),
    }
}

fn run_uninstall_with_matcher<O>(
    tool: &ToolName,
    requirement: &VersionRequirement,
    matcher: &dyn devenv_core::VersionMatcher,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut install_store = FileInstallStore::at_home(&home);
    let platform = current_platform();
    let removed = uninstall_runtime(&mut install_store, tool, requirement, platform, matcher)?;

    let Some(metadata) = removed else {
        return Err(CliError::runtime(format!(
            "{}@{} is not installed by DevEnv for {}.\nnext: run `devenv list {}` to inspect installed and registered runtimes, or use `devenv remove {} <path>` for external runtimes.",
            tool,
            requirement.raw(),
            platform.id(),
            tool,
            tool
        )));
    };

    let installation = metadata.installation();
    writeln!(
        stdout,
        "uninstalled {} {} {}",
        installation.tool(),
        installation.version(),
        installation.root().display()
    )?;

    Ok(())
}

fn run_activate_command<O>(
    args: &[String],
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if is_help_request(args) {
        write_command_help("activate", stdout)?;
        return Ok(());
    }

    if args.len() != 1 {
        return Err(CliError::usage(
            "usage: devenv activate <zsh|bash|fish|powershell>".to_owned(),
        ));
    }

    let syntax = parse_shell_syntax(&args[0])?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let _ = run_shim_rehash(context)?;
    let devenv_command = current_executable_command();
    let plan = ActivationPlan::new()
        .set_env("DEVENV_HOME", home.root().to_string_lossy())
        .prepend_path(home.shims_dir());
    let renderer = ShellActivationRenderer::new(syntax);

    writeln!(stdout, "{}", renderer.render(&plan)?)?;
    writeln!(
        stdout,
        "{}",
        render_devenv_shell_function(syntax, &devenv_command)
    )?;
    Ok(())
}

fn run_add_java<O>(path: &str, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_jdk_home(&root)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        ToolName::new("java").map_err(CoreError::from)?,
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(
        stdout,
        "added java {} {}",
        runtime.version(),
        root.display()
    )?;
    Ok(())
}

fn run_remove_java_path<O>(
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_jdk_home(&root)?;

    run_remove_java(runtime.version().raw(), Some(&root), stdout, context)
}

fn run_remove_java<O>(
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool = ToolName::new("java").map_err(CoreError::from)?;
    let version = Version::new(version).map_err(CoreError::from)?;
    let removed =
        remove_external_runtime(&mut registry, &tool, &version, current_platform(), root)?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "java@{} is not registered{}",
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed java {} {}",
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_java<O>(stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_java_runtimes(current_platform(), context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "java {} {} {} distribution={}",
            runtime.version(),
            java_source_label(runtime.source()),
            runtime.root().display(),
            runtime.distribution().as_str()
        )?;
    }

    Ok(())
}

fn run_add_go<O>(path: &str, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_go_sdk_home(&root)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        ToolName::new("go").map_err(CoreError::from)?,
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(stdout, "added go {} {}", runtime.version(), root.display())?;
    Ok(())
}

fn run_remove_go_path<O>(
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_go_sdk_home(&root)?;

    run_remove_go(runtime.version().raw(), Some(&root), stdout, context)
}

fn run_remove_go<O>(
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool = ToolName::new("go").map_err(CoreError::from)?;
    let version = Version::new(normalize_go_version(version)?).map_err(CoreError::from)?;
    let removed =
        remove_external_runtime(&mut registry, &tool, &version, current_platform(), root)?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "go@{} is not registered{}",
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed go {} {}",
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_go<O>(stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_go_runtimes(current_platform(), context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "go {} {} {}",
            runtime.version(),
            go_source_label(runtime.source()),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_add_flutter<O>(path: &str, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_flutter_sdk_home(&root)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        ToolName::new("flutter").map_err(CoreError::from)?,
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(
        stdout,
        "added flutter {} {}",
        runtime.version(),
        root.display()
    )?;
    Ok(())
}

fn run_remove_flutter_path<O>(
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_flutter_sdk_home(&root)?;

    run_remove_flutter(
        runtime.version().raw(),
        Some(runtime.root()),
        stdout,
        context,
    )
}

fn run_remove_flutter<O>(
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool = ToolName::new("flutter").map_err(CoreError::from)?;
    let version = Version::new(normalize_flutter_version(version)?).map_err(CoreError::from)?;
    let removed =
        remove_external_runtime(&mut registry, &tool, &version, current_platform(), root)?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "flutter@{} is not registered{}",
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed flutter {} {}",
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_flutter<O>(stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_flutter_runtimes(current_platform(), context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "flutter {} {} {}",
            runtime.version(),
            flutter_source_label(runtime.source()),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_add_iac<O>(
    tool: IacTool,
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_iac_tool_home(&root, tool)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        tool.tool_name(),
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(
        stdout,
        "added {} {} {}",
        tool.as_str(),
        runtime.version(),
        root.display()
    )?;
    Ok(())
}

fn run_remove_iac_path<O>(
    tool: IacTool,
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_iac_tool_home(&root, tool)?;

    run_remove_iac(
        tool,
        runtime.version().raw(),
        Some(runtime.root()),
        stdout,
        context,
    )
}

fn run_remove_iac<O>(
    tool: IacTool,
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool_name = tool.tool_name();
    let version = Version::new(normalize_iac_version(version)?).map_err(CoreError::from)?;
    let removed = remove_external_runtime(
        &mut registry,
        &tool_name,
        &version,
        current_platform(),
        root,
    )?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "{}@{} is not registered{}",
            tool.as_str(),
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed {} {} {}",
            tool.as_str(),
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_iac<O>(tool: IacTool, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_iac_runtimes(tool, current_platform(), context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "{} {} {} {}",
            tool.as_str(),
            runtime.version(),
            iac_source_label(runtime.source()),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_add_node<O>(path: &str, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_node_home(&root)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        ToolName::new("node").map_err(CoreError::from)?,
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(
        stdout,
        "added node {} {}",
        runtime.version(),
        root.display()
    )?;
    Ok(())
}

fn run_remove_node_path<O>(
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_node_home(&root)?;

    run_remove_node(
        runtime.version().raw(),
        Some(runtime.root()),
        stdout,
        context,
    )
}

fn run_remove_node<O>(
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool = ToolName::new("node").map_err(CoreError::from)?;
    let version = Version::new(normalize_node_version(version)?).map_err(CoreError::from)?;
    let removed =
        remove_external_runtime(&mut registry, &tool, &version, current_platform(), root)?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "node@{} is not registered{}",
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed node {} {}",
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_node<O>(stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_node_runtimes(current_platform(), context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "node {} {} {}",
            runtime.version(),
            node_source_label(runtime.source()),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_add_python<O>(path: &str, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_python_home(&root)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        ToolName::new("python").map_err(CoreError::from)?,
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(
        stdout,
        "added python {} {} implementation={}",
        runtime.version(),
        root.display(),
        runtime.implementation().as_str()
    )?;
    Ok(())
}

fn run_remove_python_path<O>(
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_python_home(&root)?;

    run_remove_python(
        runtime.version().raw(),
        Some(runtime.root()),
        stdout,
        context,
    )
}

fn run_remove_python<O>(
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool = ToolName::new("python").map_err(CoreError::from)?;
    let version = Version::new(normalize_python_version(version)?).map_err(CoreError::from)?;
    let removed =
        remove_external_runtime(&mut registry, &tool, &version, current_platform(), root)?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "python@{} is not registered{}",
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed python {} {}",
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_python<O>(stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_python_runtimes(current_platform(), context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "python {} {} {} implementation={}",
            runtime.version(),
            python_source_label(runtime.source()),
            runtime.root().display(),
            runtime.implementation().as_str()
        )?;
    }

    Ok(())
}

fn run_add_ruby<O>(path: &str, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_ruby_home(&root)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        ToolName::new("ruby").map_err(CoreError::from)?,
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(
        stdout,
        "added ruby {} {}",
        runtime.version(),
        root.display()
    )?;
    Ok(())
}

fn run_remove_ruby_path<O>(
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_ruby_home(&root)?;

    run_remove_ruby(
        runtime.version().raw(),
        Some(runtime.root()),
        stdout,
        context,
    )
}

fn run_remove_ruby<O>(
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool = ToolName::new("ruby").map_err(CoreError::from)?;
    let version = Version::new(normalize_ruby_version(version)?).map_err(CoreError::from)?;
    let removed =
        remove_external_runtime(&mut registry, &tool, &version, current_platform(), root)?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "ruby@{} is not registered{}",
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed ruby {} {}",
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_ruby<O>(stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_ruby_runtimes(current_platform(), context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "ruby {} {} {}",
            runtime.version(),
            ruby_source_label(runtime.source()),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_add_php<O>(path: &str, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_php_home(&root)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        ToolName::new("php").map_err(CoreError::from)?,
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(stdout, "added php {} {}", runtime.version(), root.display())?;
    Ok(())
}

fn run_remove_php_path<O>(
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_php_home(&root)?;

    run_remove_php(
        runtime.version().raw(),
        Some(runtime.root()),
        stdout,
        context,
    )
}

fn run_remove_php<O>(
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool = ToolName::new("php").map_err(CoreError::from)?;
    let version = Version::new(normalize_php_version(version)?).map_err(CoreError::from)?;
    let removed =
        remove_external_runtime(&mut registry, &tool, &version, current_platform(), root)?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "php@{} is not registered{}",
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed php {} {}",
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_php<O>(stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_php_runtimes(current_platform(), context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "php {} {} {}",
            runtime.version(),
            php_source_label(runtime.source()),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_add_rust<O>(path: &str, stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_rust_toolchain_home(&root)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let registered = RegisteredRuntime::new(
        ToolName::new("rust").map_err(CoreError::from)?,
        runtime.version().clone(),
        current_platform(),
        runtime.root(),
    );

    add_external_runtime(&mut registry, registered)?;

    writeln!(
        stdout,
        "added rust {} {}",
        runtime.version(),
        root.display()
    )?;
    Ok(())
}

fn run_remove_rust_path<O>(
    path: &str,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let root = resolve_input_path(path, &context.current_dir);
    let runtime = validate_rust_toolchain_home(&root)?;

    run_remove_rust(
        runtime.version().raw(),
        Some(runtime.root()),
        stdout,
        context,
    )
}

fn run_remove_rust<O>(
    version: &str,
    root: Option<&Path>,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut registry = FileRuntimeRegistry::at_home(&home);
    let tool = ToolName::new("rust").map_err(CoreError::from)?;
    let version = Version::new(normalize_rust_version(version)?).map_err(CoreError::from)?;
    let removed =
        remove_external_runtime(&mut registry, &tool, &version, current_platform(), root)?;

    if removed.is_empty() {
        return Err(CliError::runtime(format!(
            "rust@{} is not registered{}",
            version.raw(),
            root.map(|root| format!(" at `{}`", root.display()))
                .unwrap_or_default()
        )));
    }

    for runtime in removed {
        writeln!(
            stdout,
            "removed rust {} {}",
            runtime.version(),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn run_list_rust<O>(stdout: &mut O, context: &CommandContext) -> Result<(), CliError>
where
    O: Write,
{
    let runtimes = discover_rust_runtimes(context)?;

    for runtime in runtimes {
        writeln!(
            stdout,
            "rust {} {} {}",
            runtime.version(),
            rust_source_label(runtime.source()),
            runtime.root().display()
        )?;
    }

    Ok(())
}

fn install_request_with_resolution_metadata(
    tool: ToolName,
    requirement: &VersionRequirement,
    install_version: Version,
) -> InstallRuntimeRequest {
    let requested_spec = format!("{}@{}", tool.as_str(), requirement.raw());
    InstallRuntimeRequest::new(tool, install_version.clone(), current_platform())
        .with_metadata_field("requested_spec", requested_spec)
        .with_metadata_field("resolved_version", install_version.raw().to_owned())
}

fn write_install_dry_run<O>(
    stdout: &mut O,
    home: &DevEnvHome,
    request: &InstallRuntimeRequest,
    resolver: &dyn devenv_core::ArtifactResolver,
) -> Result<(), CliError>
where
    O: Write,
{
    let transactions = FileInstallTransactionManager::at_home(home);
    let plan = plan_install_runtime(request, resolver, &transactions)?;
    let artifact = plan.artifact();
    writeln!(stdout, "Install plan")?;
    writeln!(stdout, "tool {}", plan.tool())?;
    if let Some(requested) = request.metadata_fields().get("requested_spec") {
        writeln!(stdout, "requested {requested}")?;
    }
    writeln!(stdout, "resolved {}", plan.version())?;
    writeln!(stdout, "platform {}", plan.platform().id())?;
    writeln!(
        stdout,
        "provider {}",
        request
            .metadata_fields()
            .get("provider")
            .map(String::as_str)
            .unwrap_or("-")
    )?;
    writeln!(stdout, "url {}", artifact.url())?;
    writeln!(stdout, "checksum {}", artifact.checksum().unwrap_or("-"))?;
    writeln!(stdout, "install_path {}", plan.install_root().display())?;
    writeln!(stdout, "dry_run true")?;
    Ok(())
}

fn run_install_go<O>(
    requirement: &VersionRequirement,
    options: InstallOptions,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let loaded_metadata = load_go_release_metadata_for_install(context)?;
    let metadata = loaded_metadata.metadata;
    let tool = ToolName::new("go").map_err(CoreError::from)?;
    let source = GoReleaseVersionSource::new(metadata.clone());
    let candidates = list_remote_versions(&tool, &source)?;
    let version = GoVersionMatcher
        .match_version(requirement, &candidates)?
        .ok_or_else(|| {
            CliError::runtime(format!(
                "Go version requirement `{}` did not match available release metadata",
                requirement.raw()
            ))
        })?;
    let resolver = GoArtifactResolver::new(metadata);
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut request = install_request_with_resolution_metadata(tool, requirement, version)
        .with_metadata_field("provider", "official");
    for (key, value) in loaded_metadata.source_metadata {
        request = request.with_metadata_field(key, value);
    }
    if options.dry_run {
        return write_install_dry_run(stdout, &home, &request, &resolver);
    }
    home.create_layout()?;

    let mut downloader = CachedArtifactDownloader::at_home(&home)?;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = GoInstalledRuntimeValidator;
    let metadata = install_runtime(
        request,
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut install_store,
            lock_manager: &mut lock_manager,
            clock: &clock,
            installed_runtime_validator: Some(&validator),
        },
    )
    .map_err(|error| {
        CliError::runtime(format!(
            "failed to install go@{}: {error}\nnext: verify `{GO_RELEASE_METADATA_ENV}` or cached official Go metadata points to a readable artifact URL with a matching sha256 checksum.",
            requirement.raw()
        ))
    })?;

    writeln!(
        stdout,
        "installed go {} {}",
        metadata.installation().version(),
        metadata.installation().root().display()
    )?;
    Ok(())
}

fn run_install_flutter<O>(
    requirement: &VersionRequirement,
    options: InstallOptions,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    ensure_supported_flutter_channel(options.channel.as_deref())?;
    let requested =
        Version::new(normalize_flutter_version(requirement.raw())?).map_err(CoreError::from)?;
    let release_metadata = load_flutter_release_metadata(context)?;
    let resolver = FlutterArtifactResolver::new(release_metadata);
    let install_version = resolver.resolve_install_version(&requested)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let request = install_request_with_resolution_metadata(
        ToolName::new("flutter").map_err(CoreError::from)?,
        requirement,
        install_version,
    )
    .with_metadata_field("provider", "stable")
    .with_metadata_field("channel", "stable");
    if options.dry_run {
        return write_install_dry_run(stdout, &home, &request, &resolver);
    }
    home.create_layout()?;

    let mut downloader = CachedArtifactDownloader::at_home(&home)?;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = FlutterInstalledRuntimeValidator;
    let metadata = install_runtime(
        request,
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut install_store,
            lock_manager: &mut lock_manager,
            clock: &clock,
            installed_runtime_validator: Some(&validator),
        },
    )
    .map_err(|error| {
        CliError::runtime(format!(
            "failed to install flutter@{}: {error}\nnext: verify `{FLUTTER_RELEASE_METADATA_ENV}` points to readable metadata with a local file or file:// artifact URL, a matching sha256 checksum, and a Flutter SDK archive containing bin/flutter and bin/dart.",
            requirement.raw()
        ))
    })?;

    writeln!(
        stdout,
        "installed flutter {} {}",
        metadata.installation().version(),
        metadata.installation().root().display()
    )?;
    Ok(())
}

fn run_install_iac<O>(
    tool: IacTool,
    requirement: &VersionRequirement,
    options: InstallOptions,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let requested =
        Version::new(normalize_iac_version(requirement.raw())?).map_err(CoreError::from)?;
    let loaded_metadata = load_iac_release_metadata_for_install(tool, context)?;
    let resolver = IacArtifactResolver::new(tool, loaded_metadata.metadata);
    let install_version = resolver.resolve_install_version(&requested)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut request =
        install_request_with_resolution_metadata(tool.tool_name(), requirement, install_version)
            .with_metadata_field("provider", tool.provider_id());
    for (key, value) in loaded_metadata.source_metadata {
        request = request.with_metadata_field(key, value);
    }
    if options.dry_run {
        return write_install_dry_run(stdout, &home, &request, &resolver);
    }
    home.create_layout()?;

    let mut downloader = CachedArtifactDownloader::at_home(&home)?;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let terraform_validator = TerraformInstalledRuntimeValidator;
    let opentofu_validator = OpenTofuInstalledRuntimeValidator;
    let validator: &dyn devenv_core::InstalledRuntimeValidator = match tool {
        IacTool::Terraform => &terraform_validator,
        IacTool::OpenTofu => &opentofu_validator,
    };
    let metadata = install_runtime(
        request,
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut install_store,
            lock_manager: &mut lock_manager,
            clock: &clock,
            installed_runtime_validator: Some(validator),
        },
    )
    .map_err(|error| {
        CliError::runtime(format!(
            "failed to install {}@{}: {error}\nnext: verify `{}` points to readable metadata with a local file or file:// artifact URL, a matching sha256 checksum, and a single `{}` binary artifact.",
            tool.as_str(),
            requirement.raw(),
            iac_release_metadata_env(tool),
            tool.binary_name()
        ))
    })?;

    writeln!(
        stdout,
        "installed {} {} {}",
        tool.as_str(),
        metadata.installation().version(),
        metadata.installation().root().display()
    )?;
    Ok(())
}

fn run_install_java<O>(
    requirement: &VersionRequirement,
    options: InstallOptions,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let requested = Version::new(requirement.raw()).map_err(CoreError::from)?;
    let distribution = java_distribution_from_option(options.distribution.as_deref())?;
    ensure_supported_java_distribution(&distribution)?;
    let release_metadata = load_java_release_metadata_for_distribution(context, &distribution)?;
    let resolver = JavaArtifactResolver::with_distribution(release_metadata, distribution);
    let install_version = resolver.resolve_install_version(&requested)?;
    let distribution = resolver.distribution().as_str().to_owned();
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let request = install_request_with_resolution_metadata(
        ToolName::new("java").map_err(CoreError::from)?,
        requirement,
        install_version,
    )
    .with_metadata_field("provider", "temurin")
    .with_metadata_field("distribution", distribution.clone());
    if options.dry_run {
        return write_install_dry_run(stdout, &home, &request, &resolver);
    }
    home.create_layout()?;

    let mut downloader = CachedArtifactDownloader::at_home(&home)?;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = JavaInstalledRuntimeValidator;
    let metadata = install_runtime(
        request,
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut install_store,
            lock_manager: &mut lock_manager,
            clock: &clock,
            installed_runtime_validator: Some(&validator),
        },
    )
    .map_err(|error| {
        CliError::runtime(format!(
            "failed to install java@{}: {error}\nnext: verify `{JAVA_RELEASE_METADATA_ENV}` points to readable metadata with a local file or file:// artifact URL, a matching sha256 checksum, and a JDK archive containing bin/java and bin/javac.",
            requirement.raw()
        ))
    })?;

    writeln!(
        stdout,
        "installed java {} {} distribution={}",
        metadata.installation().version(),
        metadata.installation().root().display(),
        distribution
    )?;
    Ok(())
}

fn run_install_node<O>(
    requirement: &VersionRequirement,
    options: InstallOptions,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let requested = Version::new(requirement.raw()).map_err(CoreError::from)?;
    let loaded_metadata = load_node_release_metadata_for_install(context)?;
    let resolver = NodeArtifactResolver::new(loaded_metadata.metadata);
    let install_version = resolver.resolve_install_version(&requested)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut request = install_request_with_resolution_metadata(
        ToolName::new("node").map_err(CoreError::from)?,
        requirement,
        install_version,
    )
    .with_metadata_field("provider", "official");
    for (key, value) in loaded_metadata.source_metadata {
        request = request.with_metadata_field(key, value);
    }
    if options.dry_run {
        return write_install_dry_run(stdout, &home, &request, &resolver);
    }
    home.create_layout()?;

    let mut downloader = CachedArtifactDownloader::at_home(&home)?;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = NodeInstalledRuntimeValidator;
    let metadata = install_runtime(
        request,
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut install_store,
            lock_manager: &mut lock_manager,
            clock: &clock,
            installed_runtime_validator: Some(&validator),
        },
    )
    .map_err(|error| {
        CliError::runtime(format!(
            "failed to install node@{}: {error}\nnext: verify `{NODE_RELEASE_METADATA_ENV}` points to readable metadata with a local file or file:// artifact URL, a matching sha256 checksum, and a Node.js archive containing bin/node, bin/npm, and bin/npx.",
            requirement.raw()
        ))
    })?;

    writeln!(
        stdout,
        "installed node {} {}",
        metadata.installation().version(),
        metadata.installation().root().display()
    )?;
    Ok(())
}

fn run_install_python<O>(
    requirement: &VersionRequirement,
    options: InstallOptions,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let requested = Version::new(requirement.raw()).map_err(CoreError::from)?;
    let release_metadata = load_python_release_metadata(context)?;
    let resolver = PythonArtifactResolver::new(release_metadata);
    let install_version = resolver.resolve_install_version(&requested)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let request = install_request_with_resolution_metadata(
        ToolName::new("python").map_err(CoreError::from)?,
        requirement,
        install_version,
    )
    .with_metadata_field("implementation", "cpython");
    if options.dry_run {
        return write_install_dry_run(stdout, &home, &request, &resolver);
    }
    home.create_layout()?;

    let mut downloader = CachedArtifactDownloader::at_home(&home)?;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = PythonInstalledRuntimeValidator;
    let metadata = install_runtime(
        request,
        InstallRuntimePorts {
            artifact_resolver: &resolver,
            downloader: &mut downloader,
            checksum_verifier: &checksum,
            extractor: &mut extractor,
            transactions: &mut transactions,
            install_store: &mut install_store,
            lock_manager: &mut lock_manager,
            clock: &clock,
            installed_runtime_validator: Some(&validator),
        },
    )
    .map_err(|error| {
        CliError::runtime(format!(
            "failed to install python@{}: {error}\nnext: verify `{PYTHON_RELEASE_METADATA_ENV}` points to readable CPython metadata with a local file or file:// artifact URL, a matching sha256 checksum, and a Python archive containing bin/python, bin/python3, and bin/pip.",
            requirement.raw()
        ))
    })?;

    writeln!(
        stdout,
        "installed python {} {} implementation=cpython",
        metadata.installation().version(),
        metadata.installation().root().display()
    )?;
    Ok(())
}

fn activation_plan_for_cli_selection(
    selection: &devenv_core::ResolvedSelection,
    platform: Platform,
    context: &CommandContext,
) -> Result<ActivationPlan, CliError> {
    if let Some(runtime_root) =
        runtime_root_from_env(selection.tool(), selection.requirement(), context)
    {
        let adapter = builtin_tool_adapter(selection.tool());
        return Ok(adapter.activation_plan(&runtime_root)?);
    }

    if selection.tool().as_str() == "java" {
        let runtimes = discover_java_runtimes(platform, context)?;
        let Some(runtime) = match_java_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(JavaToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "go" {
        let runtimes = discover_go_runtimes(platform, context)?;
        let Some(runtime) = match_go_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(GoToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "flutter" {
        let runtimes = discover_flutter_runtimes(platform, context)?;
        let Some(runtime) = match_flutter_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(FlutterToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "terraform" {
        let runtimes = discover_iac_runtimes(IacTool::Terraform, platform, context)?;
        let Some(runtime) = match_iac_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(TerraformToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "opentofu" {
        let runtimes = discover_iac_runtimes(IacTool::OpenTofu, platform, context)?;
        let Some(runtime) = match_iac_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(OpenTofuToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "node" {
        let runtimes = discover_node_runtimes(platform, context)?;
        let Some(runtime) = match_node_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(NodeToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "python" {
        let runtimes = discover_python_runtimes(platform, context)?;
        let Some(runtime) = match_python_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(PythonToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "ruby" {
        let runtimes = discover_ruby_runtimes(platform, context)?;
        let Some(runtime) = match_ruby_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(RubyToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "php" {
        let runtimes = discover_php_runtimes(platform, context)?;
        let Some(runtime) = match_php_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(PhpToolAdapter::new().activation_plan(runtime.root())?);
    }

    if selection.tool().as_str() == "rust" {
        let runtimes = discover_rust_runtimes(context)?;
        let Some(runtime) = match_rust_runtime(selection.requirement(), &runtimes)? else {
            return Err(missing_runtime_error(
                selection.tool(),
                selection.requirement(),
            ));
        };

        return Ok(RustToolAdapter::new().activation_plan(runtime.root())?);
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let install_store = FileInstallStore::at_home(&home);
    let registry = FileRuntimeRegistry::at_home(&home);
    let adapter = builtin_tool_adapter(selection.tool());

    Ok(activation_plan_for_selected_runtime(
        selection.tool(),
        selection.requirement(),
        platform,
        &install_store,
        &registry,
        &adapter,
    )?)
}

fn discover_java_runtimes(
    platform: Platform,
    context: &CommandContext,
) -> Result<Vec<JavaRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(JavaRuntimeDiscovery::new()
        .with_candidate_roots(java_candidate_roots(context))
        .discover(platform, &registry, &install_store)?)
}

fn discover_go_runtimes(
    platform: Platform,
    context: &CommandContext,
) -> Result<Vec<GoRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(GoRuntimeDiscovery::new()
        .with_candidate_roots(go_candidate_roots(context))
        .discover(platform, &registry, &install_store)?)
}

fn discover_flutter_runtimes(
    platform: Platform,
    context: &CommandContext,
) -> Result<Vec<FlutterRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(FlutterRuntimeDiscovery::new()
        .with_candidate_roots(flutter_candidate_roots(context))
        .discover(platform, &registry, &install_store)?)
}

fn discover_iac_runtimes(
    tool: IacTool,
    platform: Platform,
    context: &CommandContext,
) -> Result<Vec<IacRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(IacRuntimeDiscovery::new(tool)
        .with_candidate_roots(iac_candidate_roots(tool, context))
        .discover(platform, &registry, &install_store)?)
}

fn discover_node_runtimes(
    platform: Platform,
    context: &CommandContext,
) -> Result<Vec<NodeRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(NodeRuntimeDiscovery::new()
        .with_candidate_roots(node_candidate_roots(context))
        .discover(platform, &registry, &install_store)?)
}

fn discover_python_runtimes(
    platform: Platform,
    context: &CommandContext,
) -> Result<Vec<PythonRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(PythonRuntimeDiscovery::new()
        .with_candidate_roots(python_candidate_roots(context))
        .discover(platform, &registry, &install_store)?)
}

fn discover_ruby_runtimes(
    platform: Platform,
    context: &CommandContext,
) -> Result<Vec<RubyRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(RubyRuntimeDiscovery::new()
        .with_candidate_roots(ruby_candidate_roots(context))
        .discover(platform, &registry, &install_store)?)
}

fn discover_php_runtimes(
    platform: Platform,
    context: &CommandContext,
) -> Result<Vec<PhpRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(PhpRuntimeDiscovery::new()
        .with_candidate_roots(php_candidate_roots(context))
        .discover(platform, &registry, &install_store)?)
}

fn discover_rust_runtimes(context: &CommandContext) -> Result<Vec<RustRuntime>, CliError> {
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let registry = FileRuntimeRegistry::at_home(&home);
    let install_store = FileInstallStore::at_home(&home);

    Ok(RustRuntimeDiscovery::new()
        .with_candidate_roots(rust_candidate_roots(context))
        .with_rustup_homes(rustup_homes(context))
        .discover(&registry, &install_store)?)
}

fn apply_scope<O>(
    scope: ScopeCommand,
    spec: ToolSpec,
    dry_run: bool,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    match scope {
        ScopeCommand::Local => {
            let path = context.current_dir.join("devenv.toml");
            if !dry_run {
                let mut repository = NativeConfigRepository::new(&path, ConfigScope::Project);
                repository.set_requirement(spec.tool().clone(), spec.requirement().clone())?;
                refresh_shims_if_active(context)?;
            }
            write_selection_change(
                stdout,
                "local",
                spec.tool(),
                spec.requirement(),
                Some(&path),
            )?;
            write_activation_hint_if_needed(stdout, context)
        }
        ScopeCommand::Global => {
            let path = resolve_global_config_path(context)?;
            if !dry_run {
                let mut repository = NativeConfigRepository::new(&path, ConfigScope::Global);
                repository.set_requirement(spec.tool().clone(), spec.requirement().clone())?;
                refresh_shims_if_active(context)?;
            }
            write_selection_change(
                stdout,
                "global",
                spec.tool(),
                spec.requirement(),
                Some(&path),
            )?;
            write_activation_hint_if_needed(stdout, context)
        }
        ScopeCommand::Shell => {
            if dry_run {
                writeln!(stdout, "# dry-run")?;
            }
            let key = shell_env_key(spec.tool());
            writeln!(
                stdout,
                "export {key}={}",
                shell_quote(spec.requirement().raw())
            )?;
            Ok(())
        }
    }
}

fn read_global_config(context: &CommandContext) -> Result<Option<ProjectConfig>, CliError> {
    let path = resolve_global_config_path(context)?;
    read_devenv_toml_config(&path, ConfigScope::Global).map_err(Into::into)
}

fn current_executable_command() -> String {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().into_owned())
        .unwrap_or_else(|_| COMMAND_NAME.to_owned())
}

fn render_devenv_shell_function(syntax: ShellSyntax, command: &str) -> String {
    match syntax {
        ShellSyntax::Bash | ShellSyntax::Zsh | ShellSyntax::Posix => {
            format!("devenv() {{\n  {} \"$@\"\n}}", shell_quote(command))
        }
        ShellSyntax::Fish => {
            format!("function devenv\n  {} $argv\nend", shell_quote(command))
        }
        ShellSyntax::PowerShell => {
            format!(
                "function devenv {{ & {} @args }}",
                powershell_quote(command)
            )
        }
    }
}

fn resolve_global_config_path(context: &CommandContext) -> Result<PathBuf, CliError> {
    if let Some(path) = &context.global_config_path {
        return Ok(path.clone());
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    Ok(home.global_config_file())
}

fn refresh_shims_if_active(context: &CommandContext) -> Result<(), CliError> {
    let Some(home_root) = context
        .env_vars
        .get(devenv_adapters::store::DEVENV_HOME_ENV)
        .filter(|value| !value.is_empty())
    else {
        return Ok(());
    };

    let home = DevEnvHome::new(home_root);
    if path_contains_dir(context.env_vars.get("PATH"), &home.shims_dir()) {
        let _ = run_shim_rehash(context)?;
    }
    Ok(())
}

fn write_activation_hint_if_needed<O>(
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    if let Some(home_root) = context
        .env_vars
        .get(devenv_adapters::store::DEVENV_HOME_ENV)
        .filter(|value| !value.is_empty())
    {
        let home = DevEnvHome::new(home_root);
        if path_contains_dir(context.env_vars.get("PATH"), &home.shims_dir()) {
            return Ok(());
        }
    }

    let guidance = activation_guidance(context);
    writeln!(
        stdout,
        "next: run `{}` so tool commands use DevEnv selections in this shell.",
        guidance.current_shell
    )?;
    writeln!(
        stdout,
        "new sessions: add `{}` to `{}`.",
        guidance.profile_line, guidance.profile_file
    )?;
    Ok(())
}

fn path_contains_dir(path: Option<&String>, directory: &Path) -> bool {
    path.into_iter()
        .flat_map(|value| std::env::split_paths(value))
        .any(|entry| entry == directory)
}

#[derive(Debug, Clone)]
struct ActivationGuidance {
    current_shell: String,
    profile_line: String,
    profile_file: &'static str,
}

fn activation_guidance(context: &CommandContext) -> ActivationGuidance {
    let command = shell_quote(&current_executable_command());
    let shell = context
        .env_vars
        .get("SHELL")
        .and_then(|value| Path::new(value).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("zsh");

    match shell {
        "bash" => ActivationGuidance {
            current_shell: format!("eval \"$({command} activate bash)\""),
            profile_line: format!("eval \"$({command} activate bash)\""),
            profile_file: "~/.bashrc",
        },
        "fish" => ActivationGuidance {
            current_shell: format!("{command} activate fish | source"),
            profile_line: format!("{command} activate fish | source"),
            profile_file: "~/.config/fish/config.fish",
        },
        _ => ActivationGuidance {
            current_shell: format!("eval \"$({command} activate zsh)\""),
            profile_line: format!("eval \"$({command} activate zsh)\""),
            profile_file: "~/.zshrc",
        },
    }
}

fn resolve_all_selected_tools(
    project_config: Option<&ProjectConfig>,
    global_config: Option<&ProjectConfig>,
    context: &CommandContext,
) -> Result<Vec<devenv_core::ResolvedSelection>, CliError> {
    let tools = collect_configured_tools(project_config, global_config, context.env_vars.iter());
    let mut selections = Vec::new();

    for tool in tools {
        if let Some(selection) =
            resolve_single_selection(tool, None, project_config, global_config, context)?
        {
            selections.push(selection);
        }
    }

    Ok(selections)
}

fn parse_exec_args(args: &[String]) -> Result<&[String], CliError> {
    if args.first().map(String::as_str) != Some("--") {
        return Err(CliError::usage(
            "usage: devenv exec -- <command> [args...]".to_owned(),
        ));
    }

    let command_args = &args[1..];
    if command_args.is_empty() {
        return Err(CliError::usage(
            "missing command\nusage: devenv exec -- <command> [args...]".to_owned(),
        ));
    }

    Ok(command_args)
}

fn parse_shim_dispatch_args(args: &[String]) -> Result<(&str, &[String]), CliError> {
    let binary_name = args.first().ok_or_else(|| {
        CliError::usage("usage: devenv shim dispatch <binary> -- [args...]".to_owned())
    })?;
    if binary_name.trim().is_empty() {
        return Err(CliError::usage(
            "usage: devenv shim dispatch <binary> -- [args...]".to_owned(),
        ));
    }
    if args.get(1).map(String::as_str) != Some("--") {
        return Err(CliError::usage(
            "usage: devenv shim dispatch <binary> -- [args...]".to_owned(),
        ));
    }

    Ok((binary_name, &args[2..]))
}

fn parse_shell_syntax(value: &str) -> Result<ShellSyntax, CliError> {
    match value {
        "zsh" => Ok(ShellSyntax::Zsh),
        "bash" => Ok(ShellSyntax::Bash),
        "fish" => Ok(ShellSyntax::Fish),
        "powershell" | "pwsh" => Ok(ShellSyntax::PowerShell),
        other => Err(CliError::runtime(format!(
            "unsupported shell `{other}`: expected zsh, bash, fish, or powershell"
        ))),
    }
}

fn builtin_shim_adapters() -> Vec<devenv_tools::BuiltInToolAdapter> {
    [
        "java",
        "go",
        "flutter",
        "terraform",
        "opentofu",
        "node",
        "python",
        "ruby",
        "php",
        "rust",
    ]
    .into_iter()
    .map(|tool| {
        builtin_tool_adapter(&ToolName::new(tool).expect("built-in tool name should be valid"))
    })
    .collect()
}

fn adapter_refs(adapters: &[devenv_tools::BuiltInToolAdapter]) -> Vec<&dyn ToolAdapter> {
    adapters
        .iter()
        .map(|adapter| adapter as &dyn ToolAdapter)
        .collect()
}

fn runtime_root_from_env(
    tool: &ToolName,
    requirement: &VersionRequirement,
    context: &CommandContext,
) -> Option<PathBuf> {
    context
        .env_vars
        .get(&runtime_env_key(tool, requirement))
        .map(PathBuf::from)
}

fn java_candidate_roots(context: &CommandContext) -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(value) = context.env_vars.get(JAVA_CANDIDATE_PATHS_ENV) {
        roots.extend(split_path_list(value).map(PathBuf::from));
        return roots;
    }

    if cfg!(target_os = "macos") {
        roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines"));
        if let Some(home) = context.env_vars.get("HOME") {
            roots.push(PathBuf::from(home).join("Library/Java/JavaVirtualMachines"));
        }
    } else if cfg!(target_os = "linux") {
        roots.push(PathBuf::from("/usr/lib/jvm"));
        roots.push(PathBuf::from("/usr/java"));
    }

    roots
}

fn go_candidate_roots(context: &CommandContext) -> Vec<PathBuf> {
    context
        .env_vars
        .get(GO_CANDIDATE_PATHS_ENV)
        .map(|value| split_path_list(value).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn flutter_candidate_roots(context: &CommandContext) -> Vec<PathBuf> {
    context
        .env_vars
        .get(FLUTTER_CANDIDATE_PATHS_ENV)
        .map(|value| split_path_list(value).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn iac_candidate_roots(tool: IacTool, context: &CommandContext) -> Vec<PathBuf> {
    let key = match tool {
        IacTool::Terraform => TERRAFORM_CANDIDATE_PATHS_ENV,
        IacTool::OpenTofu => OPENTOFU_CANDIDATE_PATHS_ENV,
    };

    context
        .env_vars
        .get(key)
        .map(|value| split_path_list(value).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn node_candidate_roots(context: &CommandContext) -> Vec<PathBuf> {
    context
        .env_vars
        .get(NODE_CANDIDATE_PATHS_ENV)
        .map(|value| split_path_list(value).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn python_candidate_roots(context: &CommandContext) -> Vec<PathBuf> {
    context
        .env_vars
        .get(PYTHON_CANDIDATE_PATHS_ENV)
        .map(|value| split_path_list(value).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn ruby_candidate_roots(context: &CommandContext) -> Vec<PathBuf> {
    context
        .env_vars
        .get(RUBY_CANDIDATE_PATHS_ENV)
        .map(|value| split_path_list(value).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn php_candidate_roots(context: &CommandContext) -> Vec<PathBuf> {
    context
        .env_vars
        .get(PHP_CANDIDATE_PATHS_ENV)
        .map(|value| split_path_list(value).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn rust_candidate_roots(context: &CommandContext) -> Vec<PathBuf> {
    context
        .env_vars
        .get(RUST_CANDIDATE_PATHS_ENV)
        .map(|value| split_path_list(value).map(PathBuf::from).collect())
        .unwrap_or_default()
}

fn rustup_homes(context: &CommandContext) -> Vec<PathBuf> {
    context
        .env_vars
        .get(RUSTUP_HOME_ENV)
        .filter(|value| !value.is_empty())
        .map(|value| vec![PathBuf::from(value)])
        .unwrap_or_default()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReleaseMetadataSourceKind {
    EnvFixtureOverride,
}

const FIXTURE_RELEASE_METADATA_PRIORITY: &[ReleaseMetadataSourceKind] =
    &[ReleaseMetadataSourceKind::EnvFixtureOverride];

#[derive(Debug, Clone, Copy)]
struct ReleaseMetadataInput {
    display_name: &'static str,
    env_key: &'static str,
    source_priority: &'static [ReleaseMetadataSourceKind],
}

impl ReleaseMetadataInput {
    fn fixture(display_name: &'static str, env_key: &'static str) -> Self {
        Self {
            display_name,
            env_key,
            source_priority: FIXTURE_RELEASE_METADATA_PRIORITY,
        }
    }
}

#[derive(Debug, Clone)]
struct LoadedReleaseMetadata {
    contents: String,
    source_url: String,
}

#[derive(Debug, Clone)]
struct LoadedGoReleaseMetadata {
    metadata: GoReleaseMetadata,
    source_metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct LoadedNodeReleaseMetadata {
    metadata: NodeReleaseMetadata,
    source_metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone)]
struct LoadedIacReleaseMetadata {
    metadata: IacReleaseMetadata,
    source_metadata: BTreeMap<String, String>,
}

fn load_go_release_metadata_for_install(
    context: &CommandContext,
) -> Result<LoadedGoReleaseMetadata, CliError> {
    if context.env_vars.contains_key(GO_RELEASE_METADATA_ENV) {
        let input = ReleaseMetadataInput::fixture("Go", GO_RELEASE_METADATA_ENV);
        let loaded = load_release_metadata_payload(context, input)?;
        let metadata = GoReleaseMetadata::parse(&loaded.contents).map_err(CliError::from)?;
        let source_metadata = SelectedMetadataSource::env_fixture()
            .with_metadata("source_env", input.env_key)
            .with_metadata("provider", "official")
            .validator_metadata();
        return Ok(LoadedGoReleaseMetadata {
            metadata,
            source_metadata,
        });
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let cache = FileMetadataCache::at_home(&home);
    let key = go_official_metadata_cache_key();
    if let Some(entry) = cache.read_metadata(&key)? {
        let metadata = parse_cached_go_release_metadata(entry.payload())?;
        return Ok(LoadedGoReleaseMetadata {
            metadata,
            source_metadata: entry.validator_metadata().clone(),
        });
    }

    Err(CliError::runtime(format!(
        "Go remote metadata is not configured. Set `{GO_RELEASE_METADATA_ENV}` to a fixture file, run `devenv list-remote go --refresh`, or run `devenv metadata update go`."
    )))
}

fn load_node_release_metadata_for_install(
    context: &CommandContext,
) -> Result<LoadedNodeReleaseMetadata, CliError> {
    if context.env_vars.contains_key(NODE_RELEASE_METADATA_ENV) {
        let input = ReleaseMetadataInput::fixture("Node.js", NODE_RELEASE_METADATA_ENV);
        let loaded = load_release_metadata_payload(context, input)?;
        let metadata = NodeReleaseMetadata::parse(&loaded.contents).map_err(CliError::from)?;
        let source_metadata = SelectedMetadataSource::env_fixture()
            .with_metadata("source_env", input.env_key)
            .with_metadata("provider", "official")
            .validator_metadata();
        return Ok(LoadedNodeReleaseMetadata {
            metadata,
            source_metadata,
        });
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let cache = FileMetadataCache::at_home(&home);
    let key = node_official_metadata_cache_key();
    if let Some(entry) = cache.read_metadata(&key)? {
        let metadata = parse_cached_node_release_metadata(entry.payload())?;
        return Ok(LoadedNodeReleaseMetadata {
            metadata,
            source_metadata: entry.validator_metadata().clone(),
        });
    }

    Err(CliError::runtime(format!(
        "Node.js remote metadata is not configured. Set `{NODE_RELEASE_METADATA_ENV}` to a fixture file, run `devenv list-remote node --refresh`, or run `devenv metadata update node`."
    )))
}

fn load_go_release_metadata_with_options(
    context: &CommandContext,
    options: ListRemoteOptions,
) -> Result<GoReleaseMetadata, CliError> {
    if context.env_vars.contains_key(GO_RELEASE_METADATA_ENV) {
        return load_release_metadata(
            context,
            ReleaseMetadataInput::fixture("Go", GO_RELEASE_METADATA_ENV),
            GoReleaseMetadata::parse,
        );
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut cache = FileMetadataCache::at_home(&home);
    let key = go_official_metadata_cache_key();

    if !options.refresh {
        if let Some(entry) = cache.read_metadata(&key)? {
            return parse_cached_go_release_metadata(entry.payload());
        }
    }

    if options.offline {
        return Err(CliError::runtime(format!(
            "Go remote metadata cache is missing and offline mode is enabled. Set `{GO_RELEASE_METADATA_ENV}`, run `devenv list-remote go --refresh`, or run `devenv metadata update go` before using `--offline`."
        )));
    }

    if options.refresh
        || context
            .env_vars
            .contains_key(GO_OFFICIAL_RELEASE_METADATA_ENV)
    {
        let clock = SystemClock;
        let loaded = refresh_go_official_metadata(&mut cache, &clock, context)?;
        return parse_go_official_release_metadata(&loaded.contents);
    }

    Err(CliError::runtime(format!(
        "Go remote metadata is not configured. Set `{GO_RELEASE_METADATA_ENV}` to a fixture file, run `devenv list-remote go --refresh`, or run `devenv metadata update go`."
    )))
}

fn parse_go_official_release_metadata(contents: &str) -> Result<GoReleaseMetadata, CliError> {
    GoOfficialReleaseMetadata::parse(contents)
        .and_then(GoOfficialReleaseMetadata::into_release_metadata)
        .map_err(CliError::from)
}

fn parse_go_catalog_release_metadata(contents: &str) -> Result<GoReleaseMetadata, CliError> {
    GoCatalogReleaseMetadata::parse(contents)
        .and_then(GoCatalogReleaseMetadata::into_release_metadata)
        .map_err(CliError::from)
}

fn parse_cached_go_release_metadata(contents: &str) -> Result<GoReleaseMetadata, CliError> {
    match parse_go_catalog_release_metadata(contents) {
        Ok(metadata) => Ok(metadata),
        Err(catalog_error) => match parse_go_official_release_metadata(contents) {
            Ok(metadata) => Ok(metadata),
            Err(official_error) => GoReleaseMetadata::parse(contents)
                .map_err(CliError::from)
                .map_err(|fixture_error| {
                    CliError::runtime(format!(
                        "failed to parse cached Go metadata as catalog JSON, official JSON, or fixture TOML: catalog JSON: {catalog_error}; official JSON: {official_error}; fixture TOML: {fixture_error}"
                    ))
                }),
        },
    }
}

fn refresh_go_catalog_metadata(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let catalog_root = context
        .env_vars
        .get(DEVENV_CATALOG_BASE_URL_ENV)
        .ok_or_else(|| {
            catalog_unavailable_error(format!(
                "Go catalog metadata requires `{DEVENV_CATALOG_BASE_URL_ENV}` to point at a file, http, or https catalog root"
            ))
        })?;
    let mut adapter = CatalogFetchAdapter::new(catalog_root, ReqwestMetadataHttpClient::new()?)
        .map_err(CliError::from)?;
    let request = CatalogFetchRequest::new(catalog_root);
    let trust_root = builtin_catalog_trust_root();
    let mut verifier = Sha256CatalogTrustVerifier;
    let manifest_response = adapter
        .fetch_and_verify_manifest(&request, &mut verifier, &trust_root)
        .map_err(CliError::from)?;
    let manifest_sha256 = format!("sha256:{}", hex_sha256(manifest_response.bytes()));
    let manifest = parse_catalog_manifest(manifest_response.bytes())?;
    validate_catalog_manifest_for_update(&manifest, clock)?;

    let key = go_official_metadata_cache_key();
    let entry = manifest.entry_for(&key).cloned().ok_or_else(|| {
        CliError::runtime(format!(
            "catalog manifest `{}` does not contain an entry for go/official",
            manifest.catalog_id()
        ))
    })?;
    if entry.descriptor().payload_kind() != CatalogPayloadKind::NormalizedReleaseIndex {
        return Err(CliError::runtime(format!(
            "catalog entry for go/official has unsupported payload kind `{}`",
            entry.descriptor().payload_kind().as_str()
        )));
    }

    let payload = adapter
        .fetch_entry_payload(&entry)
        .map_err(CliError::from)?;
    let source_url = payload.source_reference().to_owned();
    let contents = String::from_utf8(payload.into_bytes()).map_err(|error| {
        CliError::runtime(format!("Go catalog payload was not valid UTF-8: {error}"))
    })?;
    let catalog_metadata = GoCatalogReleaseMetadata::parse(&contents).map_err(CliError::from)?;
    if catalog_metadata.release_index().provider().as_str() != "official" {
        return Err(CliError::runtime(
            "Go catalog payload provider did not match go/official manifest entry".to_owned(),
        ));
    }
    catalog_metadata
        .into_release_metadata()
        .map_err(CliError::from)?;

    let loaded = LoadedReleaseMetadata {
        contents,
        source_url,
    };
    write_go_catalog_metadata_cache_entry(
        cache,
        clock,
        &loaded,
        &manifest,
        &entry,
        &manifest_sha256,
    )?;
    Ok(loaded)
}

fn builtin_catalog_trust_root() -> TrustRoot {
    TrustRoot::new("builtin-sha256", "sha256")
}

struct Sha256CatalogTrustVerifier;

impl CatalogTrustVerifier for Sha256CatalogTrustVerifier {
    fn verify_manifest(
        &mut self,
        manifest_bytes: &[u8],
        signature: &[u8],
        trust_root: &TrustRoot,
    ) -> devenv_core::CoreResult<CatalogVerificationResult> {
        if trust_root.id() != "builtin-sha256" {
            return Ok(CatalogVerificationResult::rejected(
                CatalogTrustFailure::UnknownTrustRoot {
                    trust_root_id: trust_root.id().to_owned(),
                },
            ));
        }
        let expected = format!("sha256:{}", hex_sha256(manifest_bytes));
        let signature = std::str::from_utf8(signature)
            .map_err(|error| {
                CoreError::message(format!("catalog signature is not UTF-8: {error}"))
            })?
            .trim();
        if signature == expected {
            Ok(CatalogVerificationResult::trusted(trust_root.id()))
        } else {
            Ok(CatalogVerificationResult::rejected(
                CatalogTrustFailure::SignatureMismatch {
                    reason: format!("expected manifest digest signature `{expected}`"),
                },
            ))
        }
    }
}

fn validate_catalog_manifest_for_update(
    manifest: &CatalogManifest,
    clock: &dyn Clock,
) -> Result<(), CliError> {
    let now = clock.now_utc()?;
    if manifest.is_expired_at(&now) {
        return Err(CliError::from(CoreError::catalog_trust(
            CatalogTrustFailure::ExpiredCatalog {
                catalog_id: manifest.catalog_id().to_owned(),
                expires_at: manifest.expires_at().to_owned(),
                now,
            },
        )));
    }
    let current_version = env!("CARGO_PKG_VERSION");
    if manifest.requires_newer_devenv(current_version) {
        return Err(CliError::from(CoreError::catalog_trust(
            CatalogTrustFailure::MinDevenvVersionMismatch {
                required: manifest.min_devenv_version().to_owned(),
                current: current_version.to_owned(),
            },
        )));
    }
    Ok(())
}

fn parse_catalog_manifest(bytes: &[u8]) -> Result<CatalogManifest, CliError> {
    let value = serde_json::from_slice::<serde_json::Value>(bytes).map_err(|error| {
        CliError::runtime(format!("failed to parse catalog manifest JSON: {error}"))
    })?;
    let schema_version = required_json_u32(&value, "schema_version", "catalog manifest")?;
    if schema_version != CATALOG_MANIFEST_SCHEMA_VERSION {
        return Err(CliError::from(CoreError::catalog_trust(
            CatalogTrustFailure::UnsupportedSchemaVersion {
                expected: CATALOG_MANIFEST_SCHEMA_VERSION,
                actual: schema_version,
            },
        )));
    }
    let entries = value
        .get("entries")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| CliError::runtime("invalid catalog manifest: missing `entries`".to_owned()))?
        .iter()
        .map(parse_catalog_manifest_entry)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(CatalogManifest::new(
        required_json_string(&value, "catalog_id", "catalog manifest")?,
        required_json_string(&value, "generated_at", "catalog manifest")?,
        required_json_string(&value, "expires_at", "catalog manifest")?,
        required_json_string(&value, "catalog_version", "catalog manifest")?,
        required_json_string(&value, "min_devenv_version", "catalog manifest")?,
        required_json_u64(&value, "sequence", "catalog manifest")?,
        entries,
    )
    .with_schema_version(schema_version))
}

fn parse_catalog_manifest_entry(value: &serde_json::Value) -> Result<CatalogEntry, CliError> {
    let tool = ToolName::new(&required_json_string(value, "tool", "catalog entry")?)
        .map_err(CoreError::from)?;
    let provider = ProviderId::new(&required_json_string(value, "provider", "catalog entry")?)
        .map_err(CoreError::from)?;
    let payload_kind = match required_json_string(value, "payload_kind", "catalog entry")?.as_str()
    {
        "normalized-release-index" => CatalogPayloadKind::NormalizedReleaseIndex,
        actual => {
            return Err(CliError::runtime(format!(
                "invalid catalog entry: unsupported payload_kind `{actual}`"
            )));
        }
    };
    let descriptor = CatalogPayloadDescriptor::new(
        required_json_string(value, "path", "catalog entry")?,
        required_json_string(value, "sha256", "catalog entry")?,
        payload_kind,
        required_json_u64(value, "ttl_seconds", "catalog entry")?,
    );

    Ok(CatalogEntry::new(
        MetadataCacheKey::new(tool, provider),
        descriptor,
    ))
}

fn required_json_string(
    value: &serde_json::Value,
    key: &str,
    context: &str,
) -> Result<String, CliError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| CliError::runtime(format!("invalid {context}: missing `{key}`")))
}

fn required_json_u32(value: &serde_json::Value, key: &str, context: &str) -> Result<u32, CliError> {
    let value = required_json_u64(value, key, context)?;
    u32::try_from(value)
        .map_err(|_| CliError::runtime(format!("invalid {context}: `{key}` is too large")))
}

fn required_json_u64(value: &serde_json::Value, key: &str, context: &str) -> Result<u64, CliError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| CliError::runtime(format!("invalid {context}: missing `{key}`")))
}

fn refresh_go_official_metadata(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    if let Some(path) = context.env_vars.get(GO_OFFICIAL_RELEASE_METADATA_ENV) {
        let input = ReleaseMetadataInput::fixture("Go official", GO_OFFICIAL_RELEASE_METADATA_ENV);
        let loaded = read_release_metadata_fixture(context, input, path)?;
        parse_go_official_release_metadata(&loaded.contents)?;
        write_go_metadata_cache_entry(
            cache,
            clock,
            &loaded,
            "official-fixture",
            Some(GO_OFFICIAL_RELEASE_METADATA_ENV),
            None,
            None,
        )?;
        return Ok(loaded);
    }

    let key = go_official_metadata_cache_key();
    let existing = cache.read_metadata(&key)?;
    let mut request = MetadataPayloadFetchRequest::new(GO_OFFICIAL_METADATA_URL);
    let reusable_existing = existing
        .as_ref()
        .filter(|entry| entry.source_url() == GO_OFFICIAL_METADATA_URL);
    if let Some(existing) = reusable_existing {
        if let Some(etag) = existing.validator_metadata().get("etag") {
            request = request.with_etag(etag.clone());
        }
        if let Some(last_modified) = existing.validator_metadata().get("last_modified") {
            request = request.with_last_modified(last_modified.clone());
        }
    }

    let mut client = ReqwestMetadataHttpClient::new()?;
    match fetch_metadata_payload(request, &mut client)? {
        MetadataFetchOutcome::Fetched(response) => {
            let etag = response.header("etag").map(ToOwned::to_owned);
            let last_modified = response.header("last-modified").map(ToOwned::to_owned);
            let contents = String::from_utf8(response.into_body()).map_err(|error| {
                CliError::runtime(format!(
                    "Go official metadata response was not valid UTF-8: {error}"
                ))
            })?;
            let loaded = LoadedReleaseMetadata {
                contents,
                source_url: GO_OFFICIAL_METADATA_URL.to_owned(),
            };
            parse_go_official_release_metadata(&loaded.contents)?;
            write_go_metadata_cache_entry(
                cache,
                clock,
                &loaded,
                "official-http",
                None,
                etag.as_deref(),
                last_modified.as_deref(),
            )?;
            Ok(loaded)
        }
        MetadataFetchOutcome::NotModified { headers } => {
            let Some(existing) = reusable_existing else {
                return Err(CliError::runtime(
                    "Go official metadata endpoint returned not-modified but no local cache exists"
                        .to_owned(),
                ));
            };
            let loaded = LoadedReleaseMetadata {
                contents: existing.payload().to_owned(),
                source_url: GO_OFFICIAL_METADATA_URL.to_owned(),
            };
            parse_go_official_release_metadata(&loaded.contents)?;
            let etag = headers
                .get("etag")
                .or_else(|| existing.validator_metadata().get("etag"))
                .map(String::as_str);
            let last_modified = headers
                .get("last-modified")
                .or_else(|| existing.validator_metadata().get("last_modified"))
                .map(String::as_str);
            write_go_metadata_cache_entry(
                cache,
                clock,
                &loaded,
                "official-http",
                None,
                etag,
                last_modified,
            )?;
            Ok(loaded)
        }
        MetadataFetchOutcome::Offline { reason } => Err(CliError::runtime(reason)),
    }
}

fn write_go_metadata_cache_entry(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    loaded: &LoadedReleaseMetadata,
    source_kind: &str,
    source_env: Option<&str>,
    etag: Option<&str>,
    last_modified: Option<&str>,
) -> Result<(), CliError> {
    let payload_sha256 = format!("sha256:{}", hex_sha256(loaded.contents.as_bytes()));
    let mut entry = MetadataCacheEntry::new(
        go_official_metadata_cache_key(),
        loaded.source_url.clone(),
        clock.now_utc()?,
        METADATA_CACHE_TTL_SECONDS,
        payload_sha256,
        MetadataPayloadKind::Raw,
        loaded.contents.clone(),
    )
    .with_validator_metadata("source_kind", source_kind)
    .with_validator_metadata("provider", "official");
    if let Some(source_env) = source_env {
        entry = entry.with_validator_metadata("source_env", source_env);
    }
    if let Some(etag) = etag {
        entry = entry.with_validator_metadata("etag", etag);
    }
    if let Some(last_modified) = last_modified {
        entry = entry.with_validator_metadata("last_modified", last_modified);
    }

    cache.write_metadata(entry)?;
    Ok(())
}

fn write_go_catalog_metadata_cache_entry(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    loaded: &LoadedReleaseMetadata,
    manifest: &CatalogManifest,
    catalog_entry: &CatalogEntry,
    manifest_sha256: &str,
) -> Result<(), CliError> {
    let selected_source = SelectedMetadataSource::catalog()
        .with_catalog_version(manifest.catalog_version())
        .with_manifest_sha256(manifest_sha256)
        .with_payload_sha256(catalog_entry.descriptor().sha256())
        .with_metadata("provider", "official");
    let mut entry = MetadataCacheEntry::new(
        go_official_metadata_cache_key(),
        loaded.source_url.clone(),
        clock.now_utc()?,
        catalog_entry.descriptor().ttl_seconds(),
        catalog_entry.descriptor().sha256().to_owned(),
        MetadataPayloadKind::Raw,
        loaded.contents.clone(),
    );
    for (key, value) in selected_source.validator_metadata() {
        entry = entry.with_validator_metadata(key, value);
    }

    cache.write_metadata(entry)?;
    Ok(())
}

fn go_official_metadata_cache_key() -> MetadataCacheKey {
    MetadataCacheKey::new(
        ToolName::new("go").expect("built-in Go tool name should be valid"),
        ProviderId::new("official").expect("built-in Go provider id should be valid"),
    )
}

fn load_flutter_release_metadata(
    context: &CommandContext,
) -> Result<FlutterReleaseMetadata, CliError> {
    load_flutter_release_metadata_with_options(context, ListRemoteOptions::default())
}

fn load_flutter_release_metadata_with_options(
    context: &CommandContext,
    options: ListRemoteOptions,
) -> Result<FlutterReleaseMetadata, CliError> {
    ensure_supported_flutter_channel(options.channel.as_deref())?;
    if context.env_vars.contains_key(FLUTTER_RELEASE_METADATA_ENV) {
        return load_release_metadata(
            context,
            ReleaseMetadataInput::fixture("Flutter", FLUTTER_RELEASE_METADATA_ENV),
            FlutterReleaseMetadata::parse,
        );
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut cache = FileMetadataCache::at_home(&home);
    let key = flutter_stable_metadata_cache_key();

    if !options.refresh {
        if let Some(entry) = cache.read_metadata(&key)? {
            return parse_cached_flutter_release_metadata(entry.payload());
        }
    }

    if options.offline {
        return Err(CliError::runtime(format!(
            "Flutter stable metadata cache is missing and offline mode is enabled. Set `{FLUTTER_RELEASE_METADATA_ENV}`, run `devenv list-remote flutter --refresh`, or run `devenv metadata update flutter` before using `--offline`."
        )));
    }

    if options.refresh
        || context
            .env_vars
            .contains_key(FLUTTER_OFFICIAL_RELEASES_DIR_ENV)
        || context.env_vars.contains_key(FLUTTER_OFFICIAL_BASE_URL_ENV)
    {
        let clock = SystemClock;
        let loaded = refresh_flutter_stable_metadata(&mut cache, &clock, context)?;
        return parse_flutter_official_metadata_bundle(&loaded.contents);
    }

    Err(CliError::runtime(format!(
        "Flutter remote metadata is not configured. Set `{FLUTTER_RELEASE_METADATA_ENV}` to a fixture TOML file, set `{FLUTTER_OFFICIAL_RELEASES_DIR_ENV}` to official release JSON fixtures, or run `devenv list-remote flutter --refresh`."
    )))
}

fn parse_cached_flutter_release_metadata(
    contents: &str,
) -> Result<FlutterReleaseMetadata, CliError> {
    match parse_flutter_official_metadata_bundle(contents) {
        Ok(metadata) => Ok(metadata),
        Err(official_error) => FlutterReleaseMetadata::parse(contents)
            .map_err(CliError::from)
            .map_err(|fixture_error| {
                CliError::runtime(format!(
                    "failed to parse cached Flutter metadata as official bundle or fixture TOML: official bundle: {official_error}; fixture TOML: {fixture_error}"
                ))
            }),
    }
}

fn parse_flutter_official_metadata_bundle(
    contents: &str,
) -> Result<FlutterReleaseMetadata, CliError> {
    let payloads = decode_flutter_official_metadata_bundle(contents)?;
    FlutterOfficialReleaseMetadata::parse_stable(&payloads)
        .and_then(FlutterOfficialReleaseMetadata::into_release_metadata)
        .map_err(CliError::from)
}

fn refresh_flutter_stable_metadata(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let loaded = if let Some(path) = context.env_vars.get(FLUTTER_OFFICIAL_RELEASES_DIR_ENV) {
        refresh_flutter_stable_metadata_from_fixture(context, path)?
    } else {
        refresh_flutter_stable_metadata_from_http(context)?
    };
    parse_flutter_official_metadata_bundle(&loaded.contents)?;
    write_flutter_stable_metadata_cache_entry(cache, clock, &loaded)?;
    Ok(loaded)
}

fn refresh_flutter_stable_metadata_from_fixture(
    context: &CommandContext,
    releases_dir: &str,
) -> Result<LoadedReleaseMetadata, CliError> {
    let releases_dir = resolve_input_path(releases_dir, &context.current_dir);
    let mut payloads = BTreeMap::new();
    for (platform, filename) in flutter_official_release_files() {
        let path = releases_dir.join(filename);
        let contents = std::fs::read_to_string(&path).map_err(|error| {
            CliError::runtime(format!(
                "failed to read Flutter official releases fixture `{}`: {error}",
                path.display()
            ))
        })?;
        payloads.insert(platform.to_owned(), contents);
    }
    let contents = encode_flutter_official_metadata_bundle(&payloads)?;

    Ok(LoadedReleaseMetadata {
        contents,
        source_url: format!("file://{}", releases_dir.display()),
    })
}

fn refresh_flutter_stable_metadata_from_http(
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let base_url = context
        .env_vars
        .get(FLUTTER_OFFICIAL_BASE_URL_ENV)
        .map(|value| value.trim_end_matches('/').to_owned())
        .unwrap_or_else(|| FLUTTER_OFFICIAL_BASE_URL.to_owned());
    let urls = if context.env_vars.contains_key(FLUTTER_OFFICIAL_BASE_URL_ENV) {
        [
            ("macos", format!("{base_url}/releases_macos.json")),
            ("linux", format!("{base_url}/releases_linux.json")),
            ("windows", format!("{base_url}/releases_windows.json")),
        ]
    } else {
        [
            ("macos", FLUTTER_OFFICIAL_MACOS_RELEASES_URL.to_owned()),
            ("linux", FLUTTER_OFFICIAL_LINUX_RELEASES_URL.to_owned()),
            ("windows", FLUTTER_OFFICIAL_WINDOWS_RELEASES_URL.to_owned()),
        ]
    };

    let mut client = ReqwestMetadataHttpClient::new()?;
    let mut payloads = BTreeMap::new();
    for (platform, url) in urls {
        let payload = fetch_flutter_official_payload(&url, &mut client)?;
        payloads.insert(platform.to_owned(), payload);
    }
    let contents = encode_flutter_official_metadata_bundle(&payloads)?;

    Ok(LoadedReleaseMetadata {
        contents,
        source_url: base_url,
    })
}

fn fetch_flutter_official_payload(
    url: &str,
    client: &mut ReqwestMetadataHttpClient,
) -> Result<String, CliError> {
    match fetch_metadata_payload(MetadataPayloadFetchRequest::new(url), client)? {
        MetadataFetchOutcome::Fetched(response) => {
            String::from_utf8(response.into_body()).map_err(|error| {
                CliError::runtime(format!(
                    "Flutter official metadata response from `{url}` was not valid UTF-8: {error}"
                ))
            })
        }
        MetadataFetchOutcome::NotModified { .. } => Err(CliError::runtime(format!(
            "Flutter official metadata endpoint `{url}` returned not-modified without a local conditional cache context"
        ))),
        MetadataFetchOutcome::Offline { reason } => Err(CliError::runtime(reason)),
    }
}

fn flutter_official_release_files() -> [(&'static str, &'static str); 3] {
    [
        ("macos", "releases_macos.json"),
        ("linux", "releases_linux.json"),
        ("windows", "releases_windows.json"),
    ]
}

fn encode_flutter_official_metadata_bundle(
    payloads: &BTreeMap<String, String>,
) -> Result<String, CliError> {
    let value = serde_json::json!({
        "schema": "devenv-flutter-stable-v1",
        "payloads": payloads,
    });
    serde_json::to_string(&value).map_err(|error| {
        CliError::runtime(format!(
            "failed to encode Flutter official metadata cache bundle: {error}"
        ))
    })
}

fn decode_flutter_official_metadata_bundle(
    contents: &str,
) -> Result<BTreeMap<String, String>, CliError> {
    let value = serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
        CliError::runtime(format!(
            "failed to parse Flutter official metadata cache bundle: {error}"
        ))
    })?;
    if value.get("schema").and_then(serde_json::Value::as_str) != Some("devenv-flutter-stable-v1") {
        return Err(CliError::runtime(
            "invalid Flutter official metadata cache bundle: unsupported schema".to_owned(),
        ));
    }
    let payloads_value = value
        .get("payloads")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            CliError::runtime(
                "invalid Flutter official metadata cache bundle: missing payloads".to_owned(),
            )
        })?;
    let mut payloads = BTreeMap::new();
    for (platform, payload) in payloads_value {
        let payload = payload.as_str().ok_or_else(|| {
            CliError::runtime(format!(
                "invalid Flutter official metadata cache bundle: payload for `{platform}` must be a string"
            ))
        })?;
        payloads.insert(platform.clone(), payload.to_owned());
    }

    Ok(payloads)
}

fn write_flutter_stable_metadata_cache_entry(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    loaded: &LoadedReleaseMetadata,
) -> Result<(), CliError> {
    let payload_sha256 = format!("sha256:{}", hex_sha256(loaded.contents.as_bytes()));
    let entry = MetadataCacheEntry::new(
        flutter_stable_metadata_cache_key(),
        loaded.source_url.clone(),
        clock.now_utc()?,
        METADATA_CACHE_TTL_SECONDS,
        payload_sha256,
        MetadataPayloadKind::Raw,
        loaded.contents.clone(),
    )
    .with_validator_metadata("source_kind", "official")
    .with_validator_metadata("provider", "stable")
    .with_validator_metadata("channel", "stable");

    cache.write_metadata(entry)?;
    Ok(())
}

fn flutter_stable_metadata_cache_key() -> MetadataCacheKey {
    MetadataCacheKey::new(
        ToolName::new("flutter").expect("built-in Flutter tool name should be valid"),
        ProviderId::new("stable").expect("built-in Flutter provider id should be valid"),
    )
}

fn load_iac_release_metadata_for_install(
    tool: IacTool,
    context: &CommandContext,
) -> Result<LoadedIacReleaseMetadata, CliError> {
    if context
        .env_vars
        .contains_key(iac_release_metadata_env(tool))
    {
        let input =
            ReleaseMetadataInput::fixture(tool.display_name(), iac_release_metadata_env(tool));
        let loaded = load_release_metadata_payload(context, input)?;
        let metadata = IacReleaseMetadata::parse(&loaded.contents).map_err(CliError::from)?;
        let source_metadata = SelectedMetadataSource::env_fixture()
            .with_metadata("source_env", input.env_key)
            .with_metadata("provider", tool.provider_id())
            .validator_metadata();
        return Ok(LoadedIacReleaseMetadata {
            metadata,
            source_metadata,
        });
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let cache = FileMetadataCache::at_home(&home);
    let key = iac_official_metadata_cache_key(tool);
    if let Some(entry) = cache.read_metadata(&key)? {
        let metadata = parse_cached_iac_release_metadata(tool, entry.payload())?;
        return Ok(LoadedIacReleaseMetadata {
            metadata,
            source_metadata: entry.validator_metadata().clone(),
        });
    }

    Err(CliError::runtime(format!(
        "{} remote metadata is not configured. Set `{}` to a fixture file, run `devenv list-remote {} --refresh`, or run `devenv metadata update {}`.",
        tool.display_name(),
        iac_release_metadata_env(tool),
        tool.as_str(),
        tool.as_str()
    )))
}

fn load_iac_release_metadata_with_options(
    tool: IacTool,
    context: &CommandContext,
    options: ListRemoteOptions,
) -> Result<IacReleaseMetadata, CliError> {
    if context
        .env_vars
        .contains_key(iac_release_metadata_env(tool))
    {
        return load_release_metadata(
            context,
            ReleaseMetadataInput::fixture(tool.display_name(), iac_release_metadata_env(tool)),
            IacReleaseMetadata::parse,
        );
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut cache = FileMetadataCache::at_home(&home);
    let key = iac_official_metadata_cache_key(tool);

    if !options.refresh {
        if let Some(entry) = cache.read_metadata(&key)? {
            return parse_cached_iac_release_metadata(tool, entry.payload());
        }
    }

    if options.offline {
        return Err(CliError::runtime(format!(
            "{} official metadata cache is missing and offline mode is enabled. Set `{}`, run `devenv list-remote {} --refresh`, or run `devenv metadata update {}` before using `--offline`.",
            tool.display_name(),
            iac_release_metadata_env(tool),
            tool.as_str(),
            tool.as_str()
        )));
    }

    if options.refresh
        || context.env_vars.contains_key(iac_official_index_env(tool))
        || context
            .env_vars
            .contains_key(iac_official_base_url_env(tool))
    {
        let clock = SystemClock;
        let loaded = refresh_iac_official_metadata(tool, &mut cache, &clock, context)?;
        return parse_iac_official_metadata_bundle(tool, &loaded.contents);
    }

    Err(CliError::runtime(format!(
        "{} remote metadata is not configured. Set `{}` to a fixture TOML file, set `{}` and `{}` to official fixtures, or run `devenv list-remote {} --refresh`.",
        tool.display_name(),
        iac_release_metadata_env(tool),
        iac_official_index_env(tool),
        iac_official_shasums_dir_env(tool),
        tool.as_str()
    )))
}

fn iac_release_metadata_env(tool: IacTool) -> &'static str {
    match tool {
        IacTool::Terraform => TERRAFORM_RELEASE_METADATA_ENV,
        IacTool::OpenTofu => OPENTOFU_RELEASE_METADATA_ENV,
    }
}

fn iac_official_index_env(tool: IacTool) -> &'static str {
    match tool {
        IacTool::Terraform => TERRAFORM_OFFICIAL_RELEASE_INDEX_ENV,
        IacTool::OpenTofu => OPENTOFU_OFFICIAL_RELEASES_ENV,
    }
}

fn iac_official_shasums_dir_env(tool: IacTool) -> &'static str {
    match tool {
        IacTool::Terraform => TERRAFORM_OFFICIAL_SHA256SUMS_DIR_ENV,
        IacTool::OpenTofu => OPENTOFU_OFFICIAL_SHA256SUMS_DIR_ENV,
    }
}

fn iac_official_base_url_env(tool: IacTool) -> &'static str {
    match tool {
        IacTool::Terraform => TERRAFORM_OFFICIAL_BASE_URL_ENV,
        IacTool::OpenTofu => OPENTOFU_OFFICIAL_BASE_URL_ENV,
    }
}

fn parse_cached_iac_release_metadata(
    tool: IacTool,
    contents: &str,
) -> Result<IacReleaseMetadata, CliError> {
    match parse_iac_catalog_release_metadata(tool, contents) {
        Ok(metadata) => Ok(metadata),
        Err(catalog_error) => match parse_iac_official_metadata_bundle(tool, contents) {
            Ok(metadata) => Ok(metadata),
            Err(official_error) => IacReleaseMetadata::parse(contents)
                .map_err(CliError::from)
                .map_err(|fixture_error| {
                    CliError::runtime(format!(
                        "failed to parse cached {} metadata as catalog JSON, official bundle, or fixture TOML: catalog JSON: {catalog_error}; official bundle: {official_error}; fixture TOML: {fixture_error}",
                        tool.display_name()
                    ))
                }),
        },
    }
}

fn parse_iac_catalog_release_metadata(
    tool: IacTool,
    contents: &str,
) -> Result<IacReleaseMetadata, CliError> {
    let metadata = match tool {
        IacTool::Terraform => IacCatalogReleaseMetadata::parse_terraform(contents),
        IacTool::OpenTofu => Err(CoreError::message(
            "OpenTofu catalog metadata path is not implemented yet".to_owned(),
        )),
    }?;
    metadata.into_release_metadata().map_err(CliError::from)
}

fn parse_iac_official_metadata_bundle(
    tool: IacTool,
    contents: &str,
) -> Result<IacReleaseMetadata, CliError> {
    let (payload, checksums_by_version) = decode_iac_official_metadata_bundle(tool, contents)?;
    parse_iac_official_release_metadata(tool, &payload, &checksums_by_version)
}

fn parse_iac_official_release_metadata(
    tool: IacTool,
    payload: &str,
    checksums_by_version: &BTreeMap<String, String>,
) -> Result<IacReleaseMetadata, CliError> {
    let metadata = match tool {
        IacTool::Terraform => {
            IacOfficialReleaseMetadata::parse_terraform(payload, checksums_by_version)
        }
        IacTool::OpenTofu => {
            IacOfficialReleaseMetadata::parse_opentofu(payload, checksums_by_version)
        }
    }?;
    metadata.into_release_metadata().map_err(CliError::from)
}

fn refresh_iac_official_metadata(
    tool: IacTool,
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let loaded = if let Some(path) = context.env_vars.get(iac_official_index_env(tool)) {
        refresh_iac_official_metadata_from_fixture(tool, context, path)?
    } else {
        refresh_iac_official_metadata_from_http(tool, context)?
    };
    parse_iac_official_metadata_bundle(tool, &loaded.contents)?;
    write_iac_official_metadata_cache_entry(tool, cache, clock, &loaded)?;
    Ok(loaded)
}

fn refresh_iac_catalog_metadata(
    tool: IacTool,
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    if tool != IacTool::Terraform {
        return Err(CliError::runtime(format!(
            "{} catalog metadata path is not implemented yet",
            tool.display_name()
        )));
    }
    let catalog_root = context
        .env_vars
        .get(DEVENV_CATALOG_BASE_URL_ENV)
        .ok_or_else(|| {
            catalog_unavailable_error(format!(
                "{} catalog metadata requires `{DEVENV_CATALOG_BASE_URL_ENV}` to point at a file, http, or https catalog root",
                tool.display_name()
            ))
        })?;
    let mut adapter = CatalogFetchAdapter::new(catalog_root, ReqwestMetadataHttpClient::new()?)
        .map_err(CliError::from)?;
    let request = CatalogFetchRequest::new(catalog_root);
    let trust_root = builtin_catalog_trust_root();
    let mut verifier = Sha256CatalogTrustVerifier;
    let manifest_response = adapter
        .fetch_and_verify_manifest(&request, &mut verifier, &trust_root)
        .map_err(CliError::from)?;
    let manifest_sha256 = format!("sha256:{}", hex_sha256(manifest_response.bytes()));
    let manifest = parse_catalog_manifest(manifest_response.bytes())?;
    validate_catalog_manifest_for_update(&manifest, clock)?;

    let key = iac_official_metadata_cache_key(tool);
    let entry = manifest.entry_for(&key).cloned().ok_or_else(|| {
        CliError::runtime(format!(
            "catalog manifest `{}` does not contain an entry for {}/{}",
            manifest.catalog_id(),
            tool.as_str(),
            tool.provider_id()
        ))
    })?;
    if entry.descriptor().payload_kind() != CatalogPayloadKind::NormalizedReleaseIndex {
        return Err(CliError::runtime(format!(
            "catalog entry for {}/{} has unsupported payload kind `{}`",
            tool.as_str(),
            tool.provider_id(),
            entry.descriptor().payload_kind().as_str()
        )));
    }

    let payload = adapter
        .fetch_entry_payload(&entry)
        .map_err(CliError::from)?;
    let source_url = payload.source_reference().to_owned();
    let contents = String::from_utf8(payload.into_bytes()).map_err(|error| {
        CliError::runtime(format!(
            "{} catalog payload was not valid UTF-8: {error}",
            tool.display_name()
        ))
    })?;
    let catalog_metadata =
        IacCatalogReleaseMetadata::parse_terraform(&contents).map_err(CliError::from)?;
    if catalog_metadata.release_index().provider().as_str() != tool.provider_id() {
        return Err(CliError::runtime(format!(
            "{} catalog payload provider did not match {}/{} manifest entry",
            tool.display_name(),
            tool.as_str(),
            tool.provider_id()
        )));
    }
    catalog_metadata
        .into_release_metadata()
        .map_err(CliError::from)?;

    let loaded = LoadedReleaseMetadata {
        contents,
        source_url,
    };
    write_iac_catalog_metadata_cache_entry(
        tool,
        cache,
        clock,
        &loaded,
        &manifest,
        &entry,
        &manifest_sha256,
    )?;
    Ok(loaded)
}

fn refresh_iac_official_metadata_from_fixture(
    tool: IacTool,
    context: &CommandContext,
    index_path: &str,
) -> Result<LoadedReleaseMetadata, CliError> {
    let input = ReleaseMetadataInput::fixture(tool.display_name(), iac_official_index_env(tool));
    let loaded = read_release_metadata_fixture(context, input, index_path)?;
    let shasums_dir = context
        .env_vars
        .get(iac_official_shasums_dir_env(tool))
        .ok_or_else(|| {
            CliError::runtime(format!(
                "{} official metadata fixture requires `{}` pointing to checksum files.",
                tool.display_name(),
                iac_official_shasums_dir_env(tool)
            ))
        })?;
    let checksums = read_iac_shasums_dir(tool, context, &loaded.contents, shasums_dir)?;
    let contents = encode_iac_official_metadata_bundle(tool, &loaded.contents, &checksums)?;

    Ok(LoadedReleaseMetadata {
        contents,
        source_url: loaded.source_url,
    })
}

fn refresh_iac_official_metadata_from_http(
    tool: IacTool,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let base_url = context
        .env_vars
        .get(iac_official_base_url_env(tool))
        .map(|value| value.trim_end_matches('/').to_owned())
        .unwrap_or_else(|| match tool {
            IacTool::Terraform => TERRAFORM_OFFICIAL_BASE_URL.to_owned(),
            IacTool::OpenTofu => OPENTOFU_OFFICIAL_BASE_URL.to_owned(),
        });
    let index_url = if context
        .env_vars
        .contains_key(iac_official_base_url_env(tool))
    {
        match tool {
            IacTool::Terraform => format!("{base_url}/index.json"),
            IacTool::OpenTofu => format!("{base_url}/releases.json"),
        }
    } else {
        match tool {
            IacTool::Terraform => TERRAFORM_OFFICIAL_INDEX_URL.to_owned(),
            IacTool::OpenTofu => OPENTOFU_OFFICIAL_RELEASES_URL.to_owned(),
        }
    };
    let mut client = ReqwestMetadataHttpClient::new()?;
    let payload = fetch_iac_official_payload(&index_url, &mut client)?;
    let versions = iac_official_required_checksum_versions(tool, &payload)?;
    let mut checksums = BTreeMap::new();
    for version in versions {
        let checksum_url = iac_official_checksum_url(tool, &base_url, &version);
        let checksum_payload = fetch_iac_official_payload(&checksum_url, &mut client)?;
        parse_iac_sha256s(&checksum_payload).map_err(CliError::from)?;
        checksums.insert(version, checksum_payload);
    }
    let contents = encode_iac_official_metadata_bundle(tool, &payload, &checksums)?;

    Ok(LoadedReleaseMetadata {
        contents,
        source_url: index_url,
    })
}

fn fetch_iac_official_payload(
    url: &str,
    client: &mut ReqwestMetadataHttpClient,
) -> Result<String, CliError> {
    match fetch_metadata_payload(MetadataPayloadFetchRequest::new(url), client)? {
        MetadataFetchOutcome::Fetched(response) => {
            String::from_utf8(response.into_body()).map_err(|error| {
                CliError::runtime(format!(
                    "IaC official metadata response from `{url}` was not valid UTF-8: {error}"
                ))
            })
        }
        MetadataFetchOutcome::NotModified { .. } => Err(CliError::runtime(format!(
            "IaC official metadata endpoint `{url}` returned not-modified without a local conditional cache context"
        ))),
        MetadataFetchOutcome::Offline { reason } => Err(CliError::runtime(reason)),
    }
}

fn read_iac_shasums_dir(
    tool: IacTool,
    context: &CommandContext,
    payload: &str,
    shasums_dir: &str,
) -> Result<BTreeMap<String, String>, CliError> {
    let shasums_dir = resolve_input_path(shasums_dir, &context.current_dir);
    let mut checksums = BTreeMap::new();
    for version in iac_official_required_checksum_versions(tool, payload)? {
        let filename = iac_official_checksum_filename(tool, &version);
        let path = [
            shasums_dir.join(&version).join(&filename),
            shasums_dir.join(format!("v{version}")).join(&filename),
            shasums_dir.join(&filename),
        ]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(|| {
            CliError::runtime(format!(
                "failed to read {} checksum fixture for {}: expected `{}` under `{}`",
                tool.display_name(),
                version,
                filename,
                shasums_dir.display()
            ))
        })?;
        let contents = std::fs::read_to_string(&path).map_err(|error| {
            CliError::runtime(format!(
                "failed to read {} checksum fixture `{}`: {error}",
                tool.display_name(),
                path.display()
            ))
        })?;
        parse_iac_sha256s(&contents).map_err(CliError::from)?;
        checksums.insert(version, contents);
    }
    Ok(checksums)
}

fn iac_official_required_checksum_versions(
    tool: IacTool,
    payload: &str,
) -> Result<Vec<String>, CliError> {
    match tool {
        IacTool::Terraform => {
            let value = serde_json::from_str::<serde_json::Value>(payload).map_err(|error| {
                CliError::runtime(format!("failed to parse Terraform official index: {error}"))
            })?;
            let versions = value
                .get("versions")
                .and_then(serde_json::Value::as_object)
                .ok_or_else(|| {
                    CliError::runtime(
                        "invalid Terraform official index: missing versions".to_owned(),
                    )
                })?
                .keys()
                .map(|version| normalize_iac_version(version).map_err(CliError::from))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(sorted_unique_iac_versions(versions))
        }
        IacTool::OpenTofu => {
            let value = serde_json::from_str::<serde_json::Value>(payload).map_err(|error| {
                CliError::runtime(format!(
                    "failed to parse OpenTofu official releases: {error}"
                ))
            })?;
            let releases = value.as_array().ok_or_else(|| {
                CliError::runtime("invalid OpenTofu official releases: expected array".to_owned())
            })?;
            let versions = releases
                .iter()
                .filter(|release| {
                    !release
                        .get("draft")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                })
                .filter_map(|release| release.get("tag_name").and_then(serde_json::Value::as_str))
                .map(|version| normalize_iac_version(version).map_err(CliError::from))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(sorted_unique_iac_versions(versions))
        }
    }
}

fn sorted_unique_iac_versions(mut versions: Vec<String>) -> Vec<String> {
    versions.sort();
    versions.dedup();
    versions
}

fn iac_official_checksum_url(tool: IacTool, base_url: &str, version: &str) -> String {
    match tool {
        IacTool::Terraform => format!(
            "{base_url}/{version}/{}",
            iac_official_checksum_filename(tool, version)
        ),
        IacTool::OpenTofu => format!(
            "{base_url}/v{version}/{}",
            iac_official_checksum_filename(tool, version)
        ),
    }
}

fn iac_official_checksum_filename(tool: IacTool, version: &str) -> String {
    match tool {
        IacTool::Terraform => format!("terraform_{version}_SHA256SUMS"),
        IacTool::OpenTofu => format!("tofu_{version}_SHA256SUMS"),
    }
}

fn encode_iac_official_metadata_bundle(
    tool: IacTool,
    payload: &str,
    checksums_by_version: &BTreeMap<String, String>,
) -> Result<String, CliError> {
    let value = serde_json::json!({
        "schema": "devenv-iac-official-v1",
        "tool": tool.as_str(),
        "payload": payload,
        "checksums": checksums_by_version,
    });
    serde_json::to_string(&value).map_err(|error| {
        CliError::runtime(format!(
            "failed to encode {} official metadata cache bundle: {error}",
            tool.display_name()
        ))
    })
}

fn decode_iac_official_metadata_bundle(
    tool: IacTool,
    contents: &str,
) -> Result<(String, BTreeMap<String, String>), CliError> {
    let value = serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
        CliError::runtime(format!(
            "failed to parse {} official metadata cache bundle: {error}",
            tool.display_name()
        ))
    })?;
    if value.get("schema").and_then(serde_json::Value::as_str) != Some("devenv-iac-official-v1") {
        return Err(CliError::runtime(format!(
            "invalid {} official metadata cache bundle: unsupported schema",
            tool.display_name()
        )));
    }
    if value.get("tool").and_then(serde_json::Value::as_str) != Some(tool.as_str()) {
        return Err(CliError::runtime(format!(
            "invalid {} official metadata cache bundle: tool mismatch",
            tool.display_name()
        )));
    }
    let payload = value
        .get("payload")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CliError::runtime(format!(
                "invalid {} official metadata cache bundle: missing payload",
                tool.display_name()
            ))
        })?
        .to_owned();
    let checksums_value = value
        .get("checksums")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            CliError::runtime(format!(
                "invalid {} official metadata cache bundle: missing checksums",
                tool.display_name()
            ))
        })?;
    let mut checksums = BTreeMap::new();
    for (version, payload) in checksums_value {
        let payload = payload.as_str().ok_or_else(|| {
            CliError::runtime(format!(
                "invalid {} official metadata cache bundle: checksums for `{version}` must be a string",
                tool.display_name()
            ))
        })?;
        checksums.insert(version.clone(), payload.to_owned());
    }

    Ok((payload, checksums))
}

fn write_iac_official_metadata_cache_entry(
    tool: IacTool,
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    loaded: &LoadedReleaseMetadata,
) -> Result<(), CliError> {
    let payload_sha256 = format!("sha256:{}", hex_sha256(loaded.contents.as_bytes()));
    let entry = MetadataCacheEntry::new(
        iac_official_metadata_cache_key(tool),
        loaded.source_url.clone(),
        clock.now_utc()?,
        METADATA_CACHE_TTL_SECONDS,
        payload_sha256,
        MetadataPayloadKind::Raw,
        loaded.contents.clone(),
    )
    .with_validator_metadata("source_kind", "official")
    .with_validator_metadata("provider", tool.provider_id());

    cache.write_metadata(entry)?;
    Ok(())
}

fn write_iac_catalog_metadata_cache_entry(
    tool: IacTool,
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    loaded: &LoadedReleaseMetadata,
    manifest: &CatalogManifest,
    catalog_entry: &CatalogEntry,
    manifest_sha256: &str,
) -> Result<(), CliError> {
    let selected_source = SelectedMetadataSource::catalog()
        .with_catalog_version(manifest.catalog_version())
        .with_manifest_sha256(manifest_sha256)
        .with_payload_sha256(catalog_entry.descriptor().sha256())
        .with_metadata("provider", tool.provider_id());
    let mut entry = MetadataCacheEntry::new(
        iac_official_metadata_cache_key(tool),
        loaded.source_url.clone(),
        clock.now_utc()?,
        catalog_entry.descriptor().ttl_seconds(),
        catalog_entry.descriptor().sha256().to_owned(),
        MetadataPayloadKind::Raw,
        loaded.contents.clone(),
    );
    for (key, value) in selected_source.validator_metadata() {
        entry = entry.with_validator_metadata(key, value);
    }

    cache.write_metadata(entry)?;
    Ok(())
}

fn iac_official_metadata_cache_key(tool: IacTool) -> MetadataCacheKey {
    MetadataCacheKey::new(
        tool.tool_name(),
        ProviderId::new(tool.provider_id()).expect("built-in IaC provider id should be valid"),
    )
}

fn load_java_release_metadata_for_distribution(
    context: &CommandContext,
    distribution: &JavaDistribution,
) -> Result<JavaReleaseMetadata, CliError> {
    load_java_release_metadata_with_options(context, distribution, ListRemoteOptions::default())
}

fn load_java_release_metadata_with_options(
    context: &CommandContext,
    distribution: &JavaDistribution,
    options: ListRemoteOptions,
) -> Result<JavaReleaseMetadata, CliError> {
    ensure_supported_java_distribution(distribution)?;

    if context.env_vars.contains_key(JAVA_RELEASE_METADATA_ENV) {
        return load_release_metadata(
            context,
            ReleaseMetadataInput::fixture("Java", JAVA_RELEASE_METADATA_ENV),
            JavaReleaseMetadata::parse,
        );
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut cache = FileMetadataCache::at_home(&home);
    let key = java_temurin_metadata_cache_key();

    if !options.refresh {
        if let Some(entry) = cache.read_metadata(&key)? {
            return parse_cached_java_release_metadata(entry.payload());
        }
    }

    if options.offline {
        return Err(CliError::runtime(format!(
            "Java Temurin metadata cache is missing and offline mode is enabled. Set `{JAVA_RELEASE_METADATA_ENV}`, run `devenv list-remote java --refresh`, or run `devenv metadata update java` before using `--offline`."
        )));
    }

    if context
        .env_vars
        .contains_key(JAVA_TEMURIN_RELEASE_METADATA_ENV)
    {
        let clock = SystemClock;
        let loaded = refresh_java_temurin_metadata_from_fixture(&mut cache, &clock, context)?;
        return parse_java_temurin_release_metadata(&loaded.contents);
    }

    let clock = SystemClock;
    let loaded = refresh_java_temurin_metadata_from_provider_manifest(&mut cache, &clock, context)?;
    parse_java_temurin_release_metadata(&loaded.contents)
}

fn parse_java_temurin_release_metadata(contents: &str) -> Result<JavaReleaseMetadata, CliError> {
    JavaTemurinReleaseMetadata::parse(contents)
        .and_then(JavaTemurinReleaseMetadata::into_release_metadata)
        .map_err(CliError::from)
}

fn parse_cached_java_release_metadata(contents: &str) -> Result<JavaReleaseMetadata, CliError> {
    match parse_java_temurin_release_metadata(contents) {
        Ok(metadata) => Ok(metadata),
        Err(temurin_error) => JavaReleaseMetadata::parse(contents)
            .map_err(CliError::from)
            .map_err(|fixture_error| {
                CliError::runtime(format!(
                    "failed to parse cached Java metadata as Temurin JSON or fixture TOML: Temurin JSON: {temurin_error}; fixture TOML: {fixture_error}"
                ))
            }),
    }
}

#[derive(Debug, Clone)]
struct JavaTemurinProviderManifest {
    available_releases_url: String,
    feature_releases_url_template: String,
    known_feature_releases: Vec<u32>,
}

impl JavaTemurinProviderManifest {
    fn feature_releases_url(&self, feature: u32, page: u32) -> String {
        self.feature_releases_url_template
            .replace("{feature}", &feature.to_string())
            .replace("{page}", &page.to_string())
    }
}

fn load_java_temurin_provider_manifest(
    context: &CommandContext,
) -> Result<JavaTemurinProviderManifest, CliError> {
    let manifest = serde_json::from_str::<serde_json::Value>(include_str!(
        "../../../metadata/providers/java/temurin/manifest.json"
    ))
    .map_err(|error| {
        CliError::runtime(format!(
            "failed to parse built-in Java Temurin provider manifest: {error}"
        ))
    })?;
    let version = manifest
        .get("version")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            CliError::runtime(
                "invalid Java Temurin provider manifest: missing `version`".to_owned(),
            )
        })?;
    let metadata = version
        .get("metadata")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            CliError::runtime(
                "invalid Java Temurin provider manifest: missing `version.metadata`".to_owned(),
            )
        })?;
    let base_url = required_manifest_string(metadata, "official_base_url")?;
    let base_url_env = required_manifest_string(metadata, "base_url_env_override")?;
    if base_url_env != JAVA_TEMURIN_API_BASE_URL_ENV {
        return Err(CliError::runtime(format!(
            "invalid Java Temurin provider manifest: base_url_env_override must be `{JAVA_TEMURIN_API_BASE_URL_ENV}`"
        )));
    }
    let base_url = context
        .env_vars
        .get(base_url_env)
        .map(String::as_str)
        .unwrap_or(base_url);
    let available_releases_path = required_manifest_string(metadata, "available_releases_path")?;
    let feature_releases_path_template =
        required_manifest_string(metadata, "feature_releases_path_template")?;
    if !feature_releases_path_template.contains("{feature}") {
        return Err(CliError::runtime(
            "invalid Java Temurin provider manifest: feature_releases_path_template must contain `{feature}`"
                .to_owned(),
        ));
    }
    if !feature_releases_path_template.contains("{page}") {
        return Err(CliError::runtime(
            "invalid Java Temurin provider manifest: feature_releases_path_template must contain `{page}`"
                .to_owned(),
        ));
    }
    let known_feature_releases = optional_manifest_u32_array(
        version
            .get("known_versions")
            .and_then(|known_versions| known_versions.get("feature_releases")),
        "version.known_versions.feature_releases",
    )?;

    Ok(JavaTemurinProviderManifest {
        available_releases_url: join_base_url_and_path(base_url, available_releases_path),
        feature_releases_url_template: join_base_url_and_path(
            base_url,
            feature_releases_path_template,
        ),
        known_feature_releases,
    })
}

fn optional_manifest_u32_array(
    value: Option<&serde_json::Value>,
    path: &str,
) -> Result<Vec<u32>, CliError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let array = value.as_array().ok_or_else(|| {
        CliError::runtime(format!(
            "invalid Java Temurin provider manifest: `{path}` must be an array"
        ))
    })?;
    let mut values = BTreeSet::new();
    for entry in array {
        let value = entry.as_u64().ok_or_else(|| {
            CliError::runtime(format!(
                "invalid Java Temurin provider manifest: `{path}` entries must be positive integers"
            ))
        })?;
        let value = u32::try_from(value).map_err(|error| {
            CliError::runtime(format!(
                "invalid Java Temurin provider manifest: `{path}` entry is too large: {error}"
            ))
        })?;
        if value == 0 {
            return Err(CliError::runtime(format!(
                "invalid Java Temurin provider manifest: `{path}` entries must be positive integers"
            )));
        }
        values.insert(value);
    }

    Ok(values.into_iter().collect())
}

fn required_manifest_string<'a>(
    metadata: &'a serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> Result<&'a str, CliError> {
    metadata
        .get(key)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            CliError::runtime(format!(
                "invalid Java Temurin provider manifest: missing `version.metadata.{key}`"
            ))
        })
}

fn join_base_url_and_path(base_url: &str, path: &str) -> String {
    format!("{}{}", base_url.trim_end_matches('/'), path)
}

fn refresh_java_temurin_metadata_from_provider_manifest(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let manifest = load_java_temurin_provider_manifest(context)?;
    let mut client = ReqwestMetadataHttpClient::new()?;
    let features = match fetch_java_temurin_metadata_url(
        &manifest.available_releases_url,
        &mut client,
        "available releases",
    ) {
        Ok(contents) => parse_java_temurin_available_releases(&contents)?,
        Err(_) if !manifest.known_feature_releases.is_empty() => {
            manifest.known_feature_releases.clone()
        }
        Err(error) => return Err(error),
    };
    let mut releases = Vec::new();

    for feature in features {
        let mut reached_last_page = false;
        for page in 0..JAVA_TEMURIN_FEATURE_RELEASE_PAGE_LIMIT {
            let url = manifest.feature_releases_url(feature, page);
            let Some(contents) =
                fetch_java_temurin_optional_metadata_url(&url, &mut client, "feature releases")?
            else {
                reached_last_page = true;
                break;
            };
            let mut feature_releases = serde_json::from_str::<Vec<serde_json::Value>>(&contents)
                .map_err(|error| {
                    CliError::runtime(format!(
                        "failed to parse Java Temurin feature metadata from `{url}`: {error}"
                    ))
                })?;
            if feature_releases.is_empty() {
                reached_last_page = true;
                break;
            }
            releases.append(&mut feature_releases);
        }
        if !reached_last_page {
            return Err(CliError::runtime(format!(
                "Java Temurin feature `{feature}` exceeded page limit {JAVA_TEMURIN_FEATURE_RELEASE_PAGE_LIMIT}; update pagination handling before trusting metadata"
            )));
        }
    }

    if releases.is_empty() {
        return Err(CliError::runtime(
            "Java Temurin provider manifest did not resolve any installable release metadata"
                .to_owned(),
        ));
    }

    let contents = serde_json::to_string(&releases).map_err(|error| {
        CliError::runtime(format!(
            "failed to serialize Java Temurin provider metadata: {error}"
        ))
    })?;
    parse_java_temurin_release_metadata(&contents)?;
    let loaded = LoadedReleaseMetadata {
        contents,
        source_url: manifest.available_releases_url,
    };
    write_java_temurin_metadata_cache_entry(cache, clock, &loaded, "official-http", None)?;
    Ok(loaded)
}

fn fetch_java_temurin_optional_metadata_url(
    url: &str,
    client: &mut ReqwestMetadataHttpClient,
    label: &str,
) -> Result<Option<String>, CliError> {
    let request = MetadataHttpRequest::new(url);
    let response = client.fetch_metadata(&request).map_err(CliError::from)?;
    match response.status() {
        200 => String::from_utf8(response.into_body())
            .map(Some)
            .map_err(|error| {
                CliError::runtime(format!(
                    "Java Temurin {label} response from `{url}` was not valid UTF-8: {error}"
                ))
            }),
        304 => Err(CliError::runtime(format!(
            "Java Temurin {label} request to `{url}` unexpectedly returned not modified without a cached payload"
        ))),
        404 => Ok(None),
        status @ 400..=499 => Err(CliError::runtime(format!(
            "Java Temurin {label} request to `{url}` failed with status {status}; provider metadata endpoint was not found or rejected the request"
        ))),
        status @ 500..=599 => Err(CliError::runtime(format!(
            "Java Temurin {label} request to `{url}` failed with status {status}; retryable=true"
        ))),
        status => Err(CliError::runtime(format!(
            "Java Temurin {label} request to `{url}` returned unsupported status {status}"
        ))),
    }
}

fn fetch_java_temurin_metadata_url(
    url: &str,
    client: &mut ReqwestMetadataHttpClient,
    label: &str,
) -> Result<String, CliError> {
    match fetch_metadata_payload(MetadataPayloadFetchRequest::new(url), client)? {
        MetadataFetchOutcome::Fetched(response) => {
            String::from_utf8(response.into_body()).map_err(|error| {
                CliError::runtime(format!(
                    "Java Temurin {label} response from `{url}` was not valid UTF-8: {error}"
                ))
            })
        }
        MetadataFetchOutcome::NotModified { .. } => Err(CliError::runtime(format!(
            "Java Temurin {label} request to `{url}` unexpectedly returned not modified without a cached payload"
        ))),
        MetadataFetchOutcome::Offline { reason } => Err(CliError::runtime(format!(
            "Java Temurin {label} request to `{url}` was skipped: {reason}"
        ))),
    }
}

fn parse_java_temurin_available_releases(contents: &str) -> Result<Vec<u32>, CliError> {
    let value = serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
        CliError::runtime(format!(
            "failed to parse Java Temurin available releases metadata: {error}"
        ))
    })?;
    let mut features = value
        .get("available_releases")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| {
            CliError::runtime(
                "invalid Java Temurin available releases metadata: missing `available_releases`"
                    .to_owned(),
            )
        })?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| {
                    CliError::runtime(
                        "invalid Java Temurin available releases metadata: release feature must be a positive integer"
                            .to_owned(),
                    )
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    features.sort_unstable_by(|left, right| right.cmp(left));
    features.dedup();
    if features.is_empty() {
        return Err(CliError::runtime(
            "Java Temurin available releases metadata did not contain any feature releases"
                .to_owned(),
        ));
    }
    Ok(features)
}

fn refresh_java_temurin_metadata_from_fixture(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let Some(path) = context.env_vars.get(JAVA_TEMURIN_RELEASE_METADATA_ENV) else {
        return Err(CliError::runtime(format!(
            "Java Temurin metadata fixture is not configured. Set `{JAVA_TEMURIN_RELEASE_METADATA_ENV}` to a Temurin API JSON fixture."
        )));
    };
    let input = ReleaseMetadataInput::fixture("Java Temurin", JAVA_TEMURIN_RELEASE_METADATA_ENV);
    let loaded = read_release_metadata_fixture(context, input, path)?;
    parse_java_temurin_release_metadata(&loaded.contents)?;
    write_java_temurin_metadata_cache_entry(
        cache,
        clock,
        &loaded,
        "temurin-fixture",
        Some(JAVA_TEMURIN_RELEASE_METADATA_ENV),
    )?;
    Ok(loaded)
}

fn write_java_temurin_metadata_cache_entry(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    loaded: &LoadedReleaseMetadata,
    source_kind: &str,
    source_env: Option<&str>,
) -> Result<(), CliError> {
    let payload_sha256 = format!("sha256:{}", hex_sha256(loaded.contents.as_bytes()));
    let mut entry = MetadataCacheEntry::new(
        java_temurin_metadata_cache_key(),
        loaded.source_url.clone(),
        clock.now_utc()?,
        METADATA_CACHE_TTL_SECONDS,
        payload_sha256,
        MetadataPayloadKind::Raw,
        loaded.contents.clone(),
    )
    .with_validator_metadata("source_kind", source_kind)
    .with_validator_metadata("provider", "temurin")
    .with_validator_metadata("distribution", "temurin");
    if let Some(source_env) = source_env {
        entry = entry.with_validator_metadata("source_env", source_env);
    }

    cache.write_metadata(entry)?;
    Ok(())
}

fn java_temurin_metadata_cache_key() -> MetadataCacheKey {
    MetadataCacheKey::new(
        ToolName::new("java").expect("built-in Java tool name should be valid"),
        ProviderId::new("temurin").expect("built-in Java provider id should be valid"),
    )
}

fn load_node_release_metadata_with_options(
    context: &CommandContext,
    options: ListRemoteOptions,
) -> Result<NodeReleaseMetadata, CliError> {
    if context.env_vars.contains_key(NODE_RELEASE_METADATA_ENV) {
        return load_release_metadata(
            context,
            ReleaseMetadataInput::fixture("Node.js", NODE_RELEASE_METADATA_ENV),
            NodeReleaseMetadata::parse,
        );
    }

    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    let mut cache = FileMetadataCache::at_home(&home);
    let key = node_official_metadata_cache_key();

    if !options.refresh {
        if let Some(entry) = cache.read_metadata(&key)? {
            return parse_cached_node_release_metadata(entry.payload());
        }
    }

    if options.offline {
        return Err(CliError::runtime(format!(
            "Node.js official metadata cache is missing and offline mode is enabled. Set `{NODE_RELEASE_METADATA_ENV}`, run `devenv list-remote node --refresh`, or run `devenv metadata update node` before using `--offline`."
        )));
    }

    if options.refresh
        || context
            .env_vars
            .contains_key(NODE_OFFICIAL_RELEASE_INDEX_ENV)
        || context.env_vars.contains_key(NODE_OFFICIAL_BASE_URL_ENV)
    {
        let clock = SystemClock;
        let loaded = refresh_node_official_metadata(&mut cache, &clock, context)?;
        return parse_node_official_metadata_bundle(&loaded.contents);
    }

    Err(CliError::runtime(format!(
        "Node.js remote metadata is not configured. Set `{NODE_RELEASE_METADATA_ENV}` to a fixture TOML file, set `{NODE_OFFICIAL_RELEASE_INDEX_ENV}` and `{NODE_OFFICIAL_SHASUMS_DIR_ENV}` to official fixtures, or run `devenv list-remote node --refresh`."
    )))
}

fn parse_cached_node_release_metadata(contents: &str) -> Result<NodeReleaseMetadata, CliError> {
    match parse_node_catalog_release_metadata(contents) {
        Ok(metadata) => Ok(metadata),
        Err(catalog_error) => match parse_node_official_metadata_bundle(contents) {
            Ok(metadata) => Ok(metadata),
            Err(official_error) => NodeReleaseMetadata::parse(contents)
                .map_err(CliError::from)
                .map_err(|fixture_error| {
                    CliError::runtime(format!(
                        "failed to parse cached Node.js metadata as catalog JSON, official bundle, or fixture TOML: catalog JSON: {catalog_error}; official bundle: {official_error}; fixture TOML: {fixture_error}"
                    ))
                }),
        },
    }
}

fn parse_node_catalog_release_metadata(contents: &str) -> Result<NodeReleaseMetadata, CliError> {
    NodeCatalogReleaseMetadata::parse(contents)
        .and_then(NodeCatalogReleaseMetadata::into_release_metadata)
        .map_err(CliError::from)
}

fn parse_node_official_metadata_bundle(contents: &str) -> Result<NodeReleaseMetadata, CliError> {
    let (index_json, shasums_by_version) = decode_node_official_metadata_bundle(contents)?;
    NodeOfficialReleaseMetadata::parse(&index_json, &shasums_by_version)
        .and_then(NodeOfficialReleaseMetadata::into_release_metadata)
        .map_err(CliError::from)
}

fn refresh_node_catalog_metadata(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let catalog_root = context
        .env_vars
        .get(DEVENV_CATALOG_BASE_URL_ENV)
        .ok_or_else(|| {
            catalog_unavailable_error(format!(
                "Node.js catalog metadata requires `{DEVENV_CATALOG_BASE_URL_ENV}` to point at a file, http, or https catalog root"
            ))
        })?;
    let mut adapter = CatalogFetchAdapter::new(catalog_root, ReqwestMetadataHttpClient::new()?)
        .map_err(CliError::from)?;
    let request = CatalogFetchRequest::new(catalog_root);
    let trust_root = builtin_catalog_trust_root();
    let mut verifier = Sha256CatalogTrustVerifier;
    let manifest_response = adapter
        .fetch_and_verify_manifest(&request, &mut verifier, &trust_root)
        .map_err(CliError::from)?;
    let manifest_sha256 = format!("sha256:{}", hex_sha256(manifest_response.bytes()));
    let manifest = parse_catalog_manifest(manifest_response.bytes())?;
    validate_catalog_manifest_for_update(&manifest, clock)?;

    let key = node_official_metadata_cache_key();
    let entry = manifest.entry_for(&key).cloned().ok_or_else(|| {
        CliError::runtime(format!(
            "catalog manifest `{}` does not contain an entry for node/official",
            manifest.catalog_id()
        ))
    })?;
    if entry.descriptor().payload_kind() != CatalogPayloadKind::NormalizedReleaseIndex {
        return Err(CliError::runtime(format!(
            "catalog entry for node/official has unsupported payload kind `{}`",
            entry.descriptor().payload_kind().as_str()
        )));
    }

    let payload = adapter
        .fetch_entry_payload(&entry)
        .map_err(CliError::from)?;
    let source_url = payload.source_reference().to_owned();
    let contents = String::from_utf8(payload.into_bytes()).map_err(|error| {
        CliError::runtime(format!(
            "Node.js catalog payload was not valid UTF-8: {error}"
        ))
    })?;
    let catalog_metadata = NodeCatalogReleaseMetadata::parse(&contents).map_err(CliError::from)?;
    if catalog_metadata.release_index().provider().as_str() != "official" {
        return Err(CliError::runtime(
            "Node.js catalog payload provider did not match node/official manifest entry"
                .to_owned(),
        ));
    }
    catalog_metadata
        .into_release_metadata()
        .map_err(CliError::from)?;

    let loaded = LoadedReleaseMetadata {
        contents,
        source_url,
    };
    write_node_catalog_metadata_cache_entry(
        cache,
        clock,
        &loaded,
        &manifest,
        &entry,
        &manifest_sha256,
    )?;
    Ok(loaded)
}

fn refresh_node_official_metadata(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let loaded = if let Some(path) = context.env_vars.get(NODE_OFFICIAL_RELEASE_INDEX_ENV) {
        refresh_node_official_metadata_from_fixture(context, path)?
    } else {
        refresh_node_official_metadata_from_http(context)?
    };
    parse_node_official_metadata_bundle(&loaded.contents)?;
    write_node_official_metadata_cache_entry(cache, clock, &loaded)?;
    Ok(loaded)
}

fn refresh_node_official_metadata_from_fixture(
    context: &CommandContext,
    index_path: &str,
) -> Result<LoadedReleaseMetadata, CliError> {
    let input = ReleaseMetadataInput::fixture("Node.js official", NODE_OFFICIAL_RELEASE_INDEX_ENV);
    let index = read_release_metadata_fixture(context, input, index_path)?;
    let shasums_dir = context.env_vars.get(NODE_OFFICIAL_SHASUMS_DIR_ENV).ok_or_else(|| {
        CliError::runtime(format!(
            "Node.js official metadata fixture requires `{NODE_OFFICIAL_SHASUMS_DIR_ENV}` pointing to a directory containing v<version>/SHASUMS256.txt files."
        ))
    })?;
    let shasums = read_node_shasums_dir(context, &index.contents, shasums_dir)?;
    let contents = encode_node_official_metadata_bundle(&index.contents, &shasums)?;

    Ok(LoadedReleaseMetadata {
        contents,
        source_url: index.source_url,
    })
}

fn refresh_node_official_metadata_from_http(
    context: &CommandContext,
) -> Result<LoadedReleaseMetadata, CliError> {
    let base_url = context
        .env_vars
        .get(NODE_OFFICIAL_BASE_URL_ENV)
        .map(|value| value.trim_end_matches('/').to_owned())
        .unwrap_or_else(|| NODE_OFFICIAL_DIST_BASE_URL.to_owned());
    let index_url = if context.env_vars.contains_key(NODE_OFFICIAL_BASE_URL_ENV) {
        format!("{base_url}/index.json")
    } else {
        NODE_OFFICIAL_INDEX_URL.to_owned()
    };
    let mut client = ReqwestMetadataHttpClient::new()?;
    let index = fetch_node_official_payload(&index_url, &mut client)?;
    let versions = node_official_required_shasums_versions(&index)?;
    let mut shasums = BTreeMap::new();
    for version in versions {
        let shasums_url = format!("{base_url}/v{version}/SHASUMS256.txt");
        let payload = fetch_node_official_payload(&shasums_url, &mut client)?;
        parse_node_shasums256(&payload).map_err(CliError::from)?;
        shasums.insert(version, payload);
    }
    let contents = encode_node_official_metadata_bundle(&index, &shasums)?;

    Ok(LoadedReleaseMetadata {
        contents,
        source_url: index_url,
    })
}

fn fetch_node_official_payload(
    url: &str,
    client: &mut ReqwestMetadataHttpClient,
) -> Result<String, CliError> {
    match fetch_metadata_payload(MetadataPayloadFetchRequest::new(url), client)? {
        MetadataFetchOutcome::Fetched(response) => {
            String::from_utf8(response.into_body()).map_err(|error| {
                CliError::runtime(format!(
                    "Node.js official metadata response from `{url}` was not valid UTF-8: {error}"
                ))
            })
        }
        MetadataFetchOutcome::NotModified { .. } => Err(CliError::runtime(format!(
            "Node.js official metadata endpoint `{url}` returned not-modified without a local conditional cache context"
        ))),
        MetadataFetchOutcome::Offline { reason } => Err(CliError::runtime(reason)),
    }
}

fn read_node_shasums_dir(
    context: &CommandContext,
    index_json: &str,
    shasums_dir: &str,
) -> Result<BTreeMap<String, String>, CliError> {
    let shasums_dir = resolve_input_path(shasums_dir, &context.current_dir);
    let mut shasums = BTreeMap::new();
    for version in node_official_required_shasums_versions(index_json)? {
        let path = shasums_dir
            .join(format!("v{version}"))
            .join("SHASUMS256.txt");
        let contents = std::fs::read_to_string(&path).map_err(|error| {
            CliError::runtime(format!(
                "failed to read Node.js SHASUMS256 fixture `{}`: {error}",
                path.display()
            ))
        })?;
        parse_node_shasums256(&contents).map_err(CliError::from)?;
        shasums.insert(version, contents);
    }
    Ok(shasums)
}

fn encode_node_official_metadata_bundle(
    index_json: &str,
    shasums_by_version: &BTreeMap<String, String>,
) -> Result<String, CliError> {
    let value = serde_json::json!({
        "schema": "devenv-node-official-v1",
        "index": index_json,
        "shasums": shasums_by_version,
    });
    serde_json::to_string(&value).map_err(|error| {
        CliError::runtime(format!(
            "failed to encode Node.js official metadata cache bundle: {error}"
        ))
    })
}

fn decode_node_official_metadata_bundle(
    contents: &str,
) -> Result<(String, BTreeMap<String, String>), CliError> {
    let value = serde_json::from_str::<serde_json::Value>(contents).map_err(|error| {
        CliError::runtime(format!(
            "failed to parse Node.js official metadata cache bundle: {error}"
        ))
    })?;
    if value.get("schema").and_then(serde_json::Value::as_str) != Some("devenv-node-official-v1") {
        return Err(CliError::runtime(
            "invalid Node.js official metadata cache bundle: unsupported schema".to_owned(),
        ));
    }
    let index = value
        .get("index")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CliError::runtime(
                "invalid Node.js official metadata cache bundle: missing index".to_owned(),
            )
        })?
        .to_owned();
    let shasums_value = value
        .get("shasums")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            CliError::runtime(
                "invalid Node.js official metadata cache bundle: missing shasums".to_owned(),
            )
        })?;
    let mut shasums = BTreeMap::new();
    for (version, payload) in shasums_value {
        let payload = payload.as_str().ok_or_else(|| {
            CliError::runtime(format!(
                "invalid Node.js official metadata cache bundle: shasums for `{version}` must be a string"
            ))
        })?;
        shasums.insert(version.clone(), payload.to_owned());
    }

    Ok((index, shasums))
}

fn write_node_official_metadata_cache_entry(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    loaded: &LoadedReleaseMetadata,
) -> Result<(), CliError> {
    let payload_sha256 = format!("sha256:{}", hex_sha256(loaded.contents.as_bytes()));
    let entry = MetadataCacheEntry::new(
        node_official_metadata_cache_key(),
        loaded.source_url.clone(),
        clock.now_utc()?,
        METADATA_CACHE_TTL_SECONDS,
        payload_sha256,
        MetadataPayloadKind::Raw,
        loaded.contents.clone(),
    )
    .with_validator_metadata("source_kind", "official")
    .with_validator_metadata("provider", "official");

    cache.write_metadata(entry)?;
    Ok(())
}

fn write_node_catalog_metadata_cache_entry(
    cache: &mut FileMetadataCache,
    clock: &dyn Clock,
    loaded: &LoadedReleaseMetadata,
    manifest: &CatalogManifest,
    catalog_entry: &CatalogEntry,
    manifest_sha256: &str,
) -> Result<(), CliError> {
    let selected_source = SelectedMetadataSource::catalog()
        .with_catalog_version(manifest.catalog_version())
        .with_manifest_sha256(manifest_sha256)
        .with_payload_sha256(catalog_entry.descriptor().sha256())
        .with_metadata("provider", "official");
    let mut entry = MetadataCacheEntry::new(
        node_official_metadata_cache_key(),
        loaded.source_url.clone(),
        clock.now_utc()?,
        catalog_entry.descriptor().ttl_seconds(),
        catalog_entry.descriptor().sha256().to_owned(),
        MetadataPayloadKind::Raw,
        loaded.contents.clone(),
    );
    for (key, value) in selected_source.validator_metadata() {
        entry = entry.with_validator_metadata(key, value);
    }

    cache.write_metadata(entry)?;
    Ok(())
}

fn node_official_metadata_cache_key() -> MetadataCacheKey {
    MetadataCacheKey::new(
        ToolName::new("node").expect("built-in Node.js tool name should be valid"),
        ProviderId::new("official").expect("built-in Node.js provider id should be valid"),
    )
}

fn load_python_release_metadata(
    context: &CommandContext,
) -> Result<PythonReleaseMetadata, CliError> {
    load_release_metadata(
        context,
        ReleaseMetadataInput::fixture("Python", PYTHON_RELEASE_METADATA_ENV),
        PythonReleaseMetadata::parse,
    )
}

fn load_release_metadata<T>(
    context: &CommandContext,
    input: ReleaseMetadataInput,
    parse: impl FnOnce(&str) -> Result<T, CoreError>,
) -> Result<T, CliError> {
    let contents = load_release_metadata_contents(context, input)?;
    parse(&contents).map_err(CliError::from)
}

fn load_release_metadata_contents(
    context: &CommandContext,
    input: ReleaseMetadataInput,
) -> Result<String, CliError> {
    Ok(load_release_metadata_payload(context, input)?.contents)
}

fn load_release_metadata_payload(
    context: &CommandContext,
    input: ReleaseMetadataInput,
) -> Result<LoadedReleaseMetadata, CliError> {
    for source in input.source_priority {
        match source {
            ReleaseMetadataSourceKind::EnvFixtureOverride => {
                if let Some(path) = context.env_vars.get(input.env_key) {
                    return read_release_metadata_fixture(context, input, path);
                }
            }
        }
    }

    Err(CliError::runtime(format!(
        "{} remote metadata is not configured. Set `{}` to a fixture file for now.",
        input.display_name, input.env_key
    )))
}

fn read_release_metadata_fixture(
    context: &CommandContext,
    input: ReleaseMetadataInput,
    path: &str,
) -> Result<LoadedReleaseMetadata, CliError> {
    let path = resolve_input_path(path, &context.current_dir);
    let contents = std::fs::read_to_string(&path).map_err(|error| {
        CliError::runtime(format!(
            "failed to read {} release metadata `{}`: {error}",
            input.display_name,
            path.display()
        ))
    })?;

    Ok(LoadedReleaseMetadata {
        contents,
        source_url: format!("file://{}", path.display()),
    })
}

fn split_path_list(value: &str) -> impl Iterator<Item = &str> {
    value
        .split(path_list_separator())
        .filter(|entry| !entry.is_empty())
}

fn path_list_separator() -> char {
    if cfg!(windows) { ';' } else { ':' }
}

fn resolve_input_path(path: &str, current_dir: &Path) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else {
        current_dir.join(path)
    }
}

fn java_source_label(source: &JavaRuntimeSource) -> &'static str {
    match source {
        JavaRuntimeSource::Registered => "registered",
        JavaRuntimeSource::Installed => "installed",
        JavaRuntimeSource::CandidatePath => "discovered",
    }
}

fn go_source_label(source: &GoRuntimeSource) -> &'static str {
    match source {
        GoRuntimeSource::Registered => "registered",
        GoRuntimeSource::Installed => "installed",
        GoRuntimeSource::CandidatePath => "discovered",
    }
}

fn flutter_source_label(source: &FlutterRuntimeSource) -> &'static str {
    match source {
        FlutterRuntimeSource::Registered => "registered",
        FlutterRuntimeSource::Installed => "installed",
        FlutterRuntimeSource::CandidatePath => "discovered",
    }
}

fn iac_source_label(source: &IacRuntimeSource) -> &'static str {
    match source {
        IacRuntimeSource::Registered => "registered",
        IacRuntimeSource::Installed => "installed",
        IacRuntimeSource::CandidatePath => "discovered",
    }
}

fn node_source_label(source: &NodeRuntimeSource) -> &'static str {
    match source {
        NodeRuntimeSource::Registered => "registered",
        NodeRuntimeSource::Installed => "installed",
        NodeRuntimeSource::CandidatePath => "discovered",
    }
}

fn python_source_label(source: &PythonRuntimeSource) -> &'static str {
    match source {
        PythonRuntimeSource::Registered => "registered",
        PythonRuntimeSource::Installed => "installed",
        PythonRuntimeSource::CandidatePath => "discovered",
    }
}

fn ruby_source_label(source: &RubyRuntimeSource) -> &'static str {
    match source {
        RubyRuntimeSource::Registered => "registered",
        RubyRuntimeSource::Installed => "installed",
        RubyRuntimeSource::CandidatePath => "discovered",
    }
}

fn php_source_label(source: &PhpRuntimeSource) -> &'static str {
    match source {
        PhpRuntimeSource::Registered => "registered",
        PhpRuntimeSource::Installed => "installed",
        PhpRuntimeSource::CandidatePath => "discovered",
    }
}

fn rust_source_label(source: &RustRuntimeSource) -> &'static str {
    match source {
        RustRuntimeSource::Registered => "registered",
        RustRuntimeSource::Installed => "installed",
        RustRuntimeSource::Rustup => "rustup",
        RustRuntimeSource::CandidatePath => "discovered",
    }
}

fn runtime_env_key(tool: &ToolName, requirement: &VersionRequirement) -> String {
    format!(
        "DEVENV_RUNTIME_{}_{}",
        env_key_fragment(tool.as_str()),
        env_key_fragment(requirement.raw())
    )
}

fn env_key_fragment(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn current_platform() -> Platform {
    let os = if cfg!(target_os = "macos") {
        OperatingSystem::Macos
    } else if cfg!(target_os = "windows") {
        OperatingSystem::Windows
    } else {
        OperatingSystem::Linux
    };
    let architecture = if cfg!(target_arch = "aarch64") {
        Architecture::Arm64
    } else {
        Architecture::X64
    };

    Platform::new(os, architecture)
}

fn write_selection_change<O>(
    stdout: &mut O,
    scope: &str,
    tool: &ToolName,
    requirement: &VersionRequirement,
    path: Option<&Path>,
) -> Result<(), CliError>
where
    O: Write,
{
    writeln!(stdout, "{tool} {} {scope}", requirement.raw())?;
    if let Some(path) = path {
        writeln!(stdout, "path: {}", path.display())?;
    }
    Ok(())
}

fn write_selection<O>(
    stdout: &mut O,
    selection: &devenv_core::ResolvedSelection,
) -> Result<(), CliError>
where
    O: Write,
{
    write!(
        stdout,
        "{} {} {}",
        selection.tool(),
        selection.requirement().raw(),
        selection.source().label()
    )?;
    if let Some(path) = selection.source_path() {
        write!(stdout, " {}", path.display())?;
    }
    writeln!(stdout)?;
    Ok(())
}

fn resolve_single_selection(
    tool: ToolName,
    cli_override: Option<VersionRequirement>,
    project_config: Option<&ProjectConfig>,
    global_config: Option<&ProjectConfig>,
    context: &CommandContext,
) -> Result<Option<devenv_core::ResolvedSelection>, CliError> {
    let mut candidates = Vec::new();

    if let Some(requirement) = cli_override {
        candidates.push(SelectionCandidate::new(
            SelectionSource::CliOverride,
            requirement,
        ));
    }

    if let Some(value) = context.env_vars.get(&shell_env_key(&tool)) {
        candidates.push(SelectionCandidate::new(
            SelectionSource::Shell,
            VersionRequirement::exact(value).map_err(CoreError::from)?,
        ));
    }

    push_config_candidate(
        &mut candidates,
        SelectionSource::Project,
        &tool,
        project_config,
    );
    push_config_candidate(
        &mut candidates,
        SelectionSource::Global,
        &tool,
        global_config,
    );

    Ok(resolve_tool_selection(tool, candidates))
}

fn push_config_candidate(
    candidates: &mut Vec<SelectionCandidate>,
    source: SelectionSource,
    tool: &ToolName,
    config: Option<&ProjectConfig>,
) {
    let Some(config) = config else {
        return;
    };
    let Some(tool_config) = config.tool(tool) else {
        return;
    };

    let mut candidate = SelectionCandidate::new(source, tool_config.requirement().clone());
    if let Some(source) = config.source() {
        candidate = candidate.with_source_path(source.path().clone());
    }
    candidates.push(candidate);
}

fn collect_configured_tools<'a>(
    project_config: Option<&ProjectConfig>,
    global_config: Option<&ProjectConfig>,
    env_vars: impl Iterator<Item = (&'a String, &'a String)>,
) -> BTreeSet<ToolName> {
    let mut tools = BTreeSet::new();

    for config in [project_config, global_config].into_iter().flatten() {
        tools.extend(config.tools().keys().cloned());
    }

    for (key, value) in env_vars {
        if value.trim().is_empty() {
            continue;
        }
        if let Some(name) = key.strip_prefix(SHELL_ENV_PREFIX) {
            if let Ok(tool) = ToolName::new(name.replace('_', "-")) {
                tools.insert(tool);
            }
        }
    }

    tools
}

fn parse_scoped_write_args(
    command_name: &str,
    args: &[String],
) -> Result<(ToolSpec, bool), CliError> {
    let usage = scoped_write_usage(command_name);
    let mut positionals = Vec::new();
    let mut dry_run = false;

    for arg in args {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            value if value.starts_with('-') => {
                return Err(CliError::usage(format!(
                    "unknown {command_name} option `{value}`\n{usage}"
                )));
            }
            value => positionals.push(value),
        }
    }

    let spec_arg = parse_tool_version_positionals(&positionals, &usage)?;
    let spec = parse_tool_spec(&spec_arg)?;

    Ok((spec, dry_run))
}

fn scoped_write_usage(command_name: &str) -> String {
    format!("usage: devenv {command_name} <tool> <version> [--dry-run]")
}

fn parse_tool_version_positionals(positionals: &[&str], usage: &str) -> Result<String, CliError> {
    match positionals {
        [spec] if spec.contains('@') => Ok((*spec).to_owned()),
        [tool, version] if !tool.contains('@') => Ok(format!("{tool}@{version}")),
        [] => Err(CliError::usage(format!(
            "missing tool and version\n{usage}"
        ))),
        _ => Err(CliError::usage(usage.to_owned())),
    }
}

fn parse_use_args(args: &[String]) -> Result<(ToolSpec, ScopeCommand, bool), CliError> {
    let usage = "usage: devenv use <tool> <version> [--scope local|global|shell] [--dry-run]";
    let mut positionals = Vec::new();
    let mut scope = ScopeCommand::Local;
    let mut dry_run = false;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--scope" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    CliError::usage("missing value for --scope: expected local, global, or shell")
                })?;
                scope = parse_scope(value)?;
                index += 2;
            }
            value if value.starts_with("--scope=") => {
                scope = parse_scope(value.trim_start_matches("--scope="))?;
                index += 1;
            }
            value if value.starts_with('-') => {
                return Err(CliError::usage(format!(
                    "unknown use option `{value}`\n{usage}"
                )));
            }
            value => {
                positionals.push(value);
                index += 1;
            }
        }
    }

    let spec_arg = parse_tool_version_positionals(&positionals, usage)?;
    let spec = parse_tool_spec(&spec_arg)?;

    Ok((spec, scope, dry_run))
}

fn parse_scope(value: &str) -> Result<ScopeCommand, CliError> {
    match value {
        "local" | "project" => Ok(ScopeCommand::Local),
        "global" => Ok(ScopeCommand::Global),
        "shell" | "session" => Ok(ScopeCommand::Shell),
        other => Err(CliError::usage(format!(
            "invalid scope `{other}`: expected local, global, or shell"
        ))),
    }
}

fn parse_tool_spec(value: &str) -> Result<ToolSpec, CliError> {
    ToolSpec::from_str(value)
        .map_err(CoreError::from)
        .map_err(Into::into)
}

fn shell_env_key(tool: &ToolName) -> String {
    format!(
        "{SHELL_ENV_PREFIX}{}",
        tool.as_str().replace('-', "_").to_ascii_uppercase()
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

fn missing_selection_error(tool: &ToolName) -> CliError {
    CliError::runtime(format!(
        "no version selected for {tool}. Run `devenv local {tool} <version>`, `devenv global {tool} <version>`, or `eval \"$(devenv shell {tool} <version>)\"`."
    ))
}

fn missing_runtime_error(tool: &ToolName, requirement: &VersionRequirement) -> CliError {
    CliError::runtime(format!(
        "{} {} is selected but not installed or registered.\nRun `devenv add {} <path>` for an existing runtime, `devenv install {} {}` for a DevEnv-owned runtime, or `devenv list {}` to inspect known runtimes.",
        tool,
        requirement.raw(),
        tool,
        tool,
        requirement.raw(),
        tool
    ))
}

#[derive(Debug, Clone)]
struct DoctorReport {
    checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    fn collect(context: &CommandContext) -> Self {
        let mut checks = Vec::new();

        let home = match DevEnvHome::resolve_from_env(&context.env_vars) {
            Ok(home) => {
                checks.push(DoctorCheck::ok(
                    "DEVENV_HOME",
                    format!("resolved to {}", home.root().display()),
                    Some(home.root().to_path_buf()),
                    None,
                ));
                home
            }
            Err(error) => {
                checks.push(DoctorCheck::error(
                    "DEVENV_HOME",
                    error.to_string(),
                    None,
                    Some("set DEVENV_HOME or HOME before running DevEnv"),
                ));
                return Self { checks };
            }
        };

        let install_store = FileInstallStore::at_home(&home);
        if home.installs_dir().is_dir() {
            match count_install_metadata(&install_store) {
                Ok(count) => checks.push(DoctorCheck::ok(
                    "install store",
                    format!("{count} installed runtime metadata entries"),
                    Some(home.installs_dir()),
                    None,
                )),
                Err(error) => checks.push(DoctorCheck::error(
                    "install store",
                    error.to_string(),
                    Some(home.installs_dir()),
                    Some("inspect or remove invalid devenv-install.toml files"),
                )),
            }
        } else {
            checks.push(DoctorCheck::warning(
                "install store",
                "install store directory does not exist yet",
                Some(home.installs_dir()),
                Some("run `devenv install <tool> <version>` or `devenv shim init`"),
            ));
        }

        let registry = FileRuntimeRegistry::at_home(&home);
        if let Some(parent) = home.external_registry_file().parent() {
            if parent.exists() && !parent.is_dir() {
                checks.push(DoctorCheck::error(
                    "runtime registry",
                    "registry parent exists but is not a directory",
                    Some(parent.to_path_buf()),
                    Some("move the file and run `devenv add <tool> <path>` again"),
                ));
            } else {
                match registry.list_all() {
                    Ok(runtimes) => checks.push(DoctorCheck::ok(
                        "runtime registry",
                        format!("{} registered external runtimes", runtimes.len()),
                        Some(home.external_registry_file()),
                        None,
                    )),
                    Err(error) => checks.push(DoctorCheck::error(
                        "runtime registry",
                        error.to_string(),
                        Some(home.external_registry_file()),
                        Some("fix or remove the runtime registry file"),
                    )),
                }
            }
        }

        let shim_dir = home.shims_dir();
        if shim_dir.is_dir() {
            let adapters = builtin_shim_adapters();
            let adapter_refs = adapter_refs(&adapters);
            match collect_shim_specs(&adapter_refs) {
                Ok(specs) => {
                    let missing = specs
                        .iter()
                        .filter(|spec| !shim_dir.join(spec.binary_name()).is_file())
                        .count();
                    if missing == 0 {
                        checks.push(DoctorCheck::ok(
                            "shim directory",
                            format!("{} expected shims present", specs.len()),
                            Some(shim_dir),
                            None,
                        ));
                    } else {
                        checks.push(DoctorCheck::warning(
                            "shim directory",
                            format!("{missing} expected shims are missing"),
                            Some(shim_dir),
                            Some("run `devenv shim rehash`"),
                        ));
                    }
                }
                Err(error) => checks.push(DoctorCheck::error(
                    "shim directory",
                    error.to_string(),
                    Some(shim_dir),
                    Some("fix adapter exposed binary metadata"),
                )),
            }
        } else {
            checks.push(DoctorCheck::warning(
                "shim directory",
                "shim directory does not exist",
                Some(shim_dir),
                Some("run `devenv shim init`"),
            ));
        }

        match discover_project_config_from(&context.current_dir) {
            Ok(Some(config)) => {
                let path = config.source().map(|source| source.path().clone());
                checks.push(DoctorCheck::ok(
                    "project config",
                    "project config discovered",
                    path,
                    None,
                ));
            }
            Ok(None) => checks.push(DoctorCheck::ok(
                "project config",
                "no project config found from current directory",
                Some(context.current_dir.clone()),
                Some("run `devenv local <tool> <version>` to create one"),
            )),
            Err(error) => checks.push(DoctorCheck::error(
                "project config",
                error.to_string(),
                Some(context.current_dir.clone()),
                Some("fix the nearest devenv.toml file"),
            )),
        }

        match resolve_global_config_path(context) {
            Ok(path) => match read_devenv_toml_config(&path, ConfigScope::Global) {
                Ok(Some(_)) => checks.push(DoctorCheck::ok(
                    "global config",
                    "global config is readable",
                    Some(path),
                    None,
                )),
                Ok(None) if context.global_config_path.is_some() => {
                    checks.push(DoctorCheck::warning(
                        "global config",
                        "global config path is configured but file does not exist",
                        Some(path),
                        Some("run `devenv global <tool> <version>`"),
                    ));
                }
                Ok(None) => checks.push(DoctorCheck::ok(
                    "global config",
                    "default global config has not been created yet",
                    Some(path),
                    Some("run `devenv global <tool> <version>` to create one"),
                )),
                Err(error) => {
                    let next = if context.global_config_path.is_some() {
                        "fix or remove DEVENV_GLOBAL_CONFIG"
                    } else {
                        "fix or remove the default global config file"
                    };
                    checks.push(DoctorCheck::error(
                        "global config",
                        error.to_string(),
                        Some(path),
                        Some(next),
                    ));
                }
            },
            Err(error) => checks.push(DoctorCheck::error(
                "global config",
                error.to_string(),
                None,
                Some("set DEVENV_HOME or HOME to a writable location"),
            )),
        }

        Self { checks }
    }

    fn status(&self) -> DoctorStatus {
        if self
            .checks
            .iter()
            .any(|check| check.status == DoctorStatus::Error)
        {
            DoctorStatus::Error
        } else if self
            .checks
            .iter()
            .any(|check| check.status == DoctorStatus::Warning)
        {
            DoctorStatus::Warning
        } else {
            DoctorStatus::Ok
        }
    }

    fn has_errors(&self) -> bool {
        self.status() == DoctorStatus::Error
    }

    fn write_text<O: Write>(&self, stdout: &mut O) -> Result<(), CliError> {
        writeln!(stdout, "DevEnv doctor")?;
        writeln!(stdout, "status: {}", self.status().as_str())?;
        for check in &self.checks {
            writeln!(
                stdout,
                "[{}] {}: {}",
                check.status.as_str(),
                check.name,
                check.message
            )?;
            if let Some(path) = &check.path {
                writeln!(stdout, "path: {}", path.display())?;
            }
            if let Some(guidance) = &check.guidance {
                writeln!(stdout, "next: {guidance}")?;
            }
        }

        Ok(())
    }

    fn to_json(&self) -> String {
        let checks = self
            .checks
            .iter()
            .map(DoctorCheck::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"status\":\"{}\",\"checks\":[{}]}}",
            self.status().as_str(),
            checks
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DoctorCheck {
    name: String,
    status: DoctorStatus,
    message: String,
    path: Option<PathBuf>,
    guidance: Option<String>,
}

impl DoctorCheck {
    fn ok(
        name: impl Into<String>,
        message: impl Into<String>,
        path: Option<PathBuf>,
        guidance: Option<&str>,
    ) -> Self {
        Self::new(name, DoctorStatus::Ok, message, path, guidance)
    }

    fn warning(
        name: impl Into<String>,
        message: impl Into<String>,
        path: Option<PathBuf>,
        guidance: Option<&str>,
    ) -> Self {
        Self::new(name, DoctorStatus::Warning, message, path, guidance)
    }

    fn error(
        name: impl Into<String>,
        message: impl Into<String>,
        path: Option<PathBuf>,
        guidance: Option<&str>,
    ) -> Self {
        Self::new(name, DoctorStatus::Error, message, path, guidance)
    }

    fn new(
        name: impl Into<String>,
        status: DoctorStatus,
        message: impl Into<String>,
        path: Option<PathBuf>,
        guidance: Option<&str>,
    ) -> Self {
        Self {
            name: name.into(),
            status,
            message: message.into(),
            path,
            guidance: guidance.map(ToOwned::to_owned),
        }
    }

    fn to_json(&self) -> String {
        let mut fields = vec![
            format!("\"name\":\"{}\"", escape_json(&self.name)),
            format!("\"status\":\"{}\"", self.status.as_str()),
            format!("\"message\":\"{}\"", escape_json(&self.message)),
        ];
        if let Some(path) = &self.path {
            fields.push(format!(
                "\"path\":\"{}\"",
                escape_json(&path.to_string_lossy())
            ));
        }
        if let Some(guidance) = &self.guidance {
            fields.push(format!("\"guidance\":\"{}\"", escape_json(guidance)));
        }

        format!("{{{}}}", fields.join(","))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DoctorStatus {
    Ok,
    Warning,
    Error,
}

impl DoctorStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

fn count_install_metadata(store: &FileInstallStore) -> Result<usize, CliError> {
    let tools = [
        "java",
        "go",
        "flutter",
        "terraform",
        "opentofu",
        "node",
        "python",
        "ruby",
        "php",
        "rust",
    ]
    .into_iter()
    .map(ToolName::new)
    .collect::<Result<Vec<_>, _>>()
    .map_err(CoreError::from)?;
    let mut count = 0;
    for tool in tools {
        count += store.list_installation_metadata(&tool)?.len();
    }

    Ok(count)
}

fn escape_json(value: &str) -> String {
    let mut output = String::new();
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character.is_control() => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }

    output
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ScopeCommand {
    Local,
    Global,
    Shell,
}

impl ScopeCommand {
    fn command_name(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Global => "global",
            Self::Shell => "shell",
        }
    }
}

#[derive(Debug, Clone)]
struct CommandContext {
    current_dir: PathBuf,
    global_config_path: Option<PathBuf>,
    env_vars: std::collections::BTreeMap<String, String>,
}

impl CommandContext {
    fn from_env() -> Self {
        Self {
            current_dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            global_config_path: std::env::var_os(GLOBAL_CONFIG_ENV).map(PathBuf::from),
            env_vars: std::env::vars().collect(),
        }
    }
}

#[derive(Debug)]
enum CliError {
    Usage(String),
    Runtime(String),
}

impl CliError {
    fn usage(message: impl Into<String>) -> Self {
        Self::Usage(message.into())
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self::Runtime(message.into())
    }

    fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) => 2,
            Self::Runtime(_) => 1,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) | Self::Runtime(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CliError {}

impl From<CoreError> for CliError {
    fn from(value: CoreError) -> Self {
        match value {
            CoreError::CatalogTrust(failure) => catalog_trust_failure_error(failure),
            CoreError::CatalogNetwork(message) => catalog_network_failure_error(message),
            other => Self::Runtime(other.to_string()),
        }
    }
}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        Self::Runtime(value.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::metadata_source::*;
    use super::*;

    fn test_context(catalog_gate: Option<&str>) -> CommandContext {
        let mut env_vars = BTreeMap::new();
        if let Some(value) = catalog_gate {
            env_vars.insert(DEVENV_ENABLE_CATALOG_ENV.to_owned(), value.to_owned());
        }
        CommandContext {
            current_dir: PathBuf::from("."),
            global_config_path: None,
            env_vars,
        }
    }

    fn catalog_trust_failure() -> CatalogTrustFailure {
        CatalogTrustFailure::SignatureMismatch {
            reason: "test signature mismatch".to_owned(),
        }
    }

    #[test]
    fn metadata_source_parses_supported_modes() {
        assert_eq!(
            parse_metadata_source_mode("auto").expect("auto should parse"),
            MetadataSourceMode::Auto
        );
        assert_eq!(
            parse_metadata_source_mode("env").expect("env should parse"),
            MetadataSourceMode::Env
        );
        assert_eq!(
            parse_metadata_source_mode("cache").expect("cache should parse"),
            MetadataSourceMode::Cache
        );
        assert_eq!(
            parse_metadata_source_mode("catalog").expect("catalog should parse"),
            MetadataSourceMode::Catalog
        );
        assert_eq!(
            parse_metadata_source_mode("official").expect("official should parse"),
            MetadataSourceMode::Official
        );
        assert!(matches!(
            parse_metadata_source_mode("unknown"),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn metadata_source_env_override_has_highest_precedence() {
        let selected = resolve_metadata_source_selection(
            MetadataSourceResolutionRequest::new(MetadataSourceMode::Auto).catalog_enabled(true),
            MetadataSourceCandidates {
                env_override: Some(SelectedMetadataSource::env_fixture()),
                cache: Some(SelectedMetadataSource::cache()),
                catalog: MetadataSourceCandidate::available(SelectedMetadataSource::catalog()),
                official: MetadataSourceCandidate::available(SelectedMetadataSource::official()),
                stale_cache: Some(SelectedMetadataSource::stale_cache()),
            },
        )
        .expect("env source should win");

        assert_eq!(selected.kind(), SelectedMetadataSourceKind::EnvFixture);
    }

    #[test]
    fn metadata_source_offline_catalog_combination_is_invalid() {
        let error = resolve_metadata_source_selection(
            MetadataSourceResolutionRequest::new(MetadataSourceMode::Catalog).offline(true),
            MetadataSourceCandidates::default(),
        )
        .expect_err("offline catalog source should be invalid");

        assert!(matches!(
            error,
            MetadataSourceResolutionError::Validation(message)
                if message.contains("--offline") && message.contains("--source catalog")
        ));
    }

    #[test]
    fn metadata_source_env_mode_does_not_use_network_candidates() {
        let error = resolve_metadata_source_selection(
            MetadataSourceResolutionRequest::new(MetadataSourceMode::Env).catalog_enabled(true),
            MetadataSourceCandidates {
                catalog: MetadataSourceCandidate::network_failure("catalog should not be touched"),
                official: MetadataSourceCandidate::available(SelectedMetadataSource::official()),
                ..MetadataSourceCandidates::default()
            },
        )
        .expect_err("missing env fixture should stop at env source");

        assert!(matches!(
            error,
            MetadataSourceResolutionError::Unavailable(message)
                if message.contains("env fixture")
        ));
    }

    #[test]
    fn metadata_source_cache_mode_reports_cache_miss() {
        let error = resolve_metadata_source_selection(
            MetadataSourceResolutionRequest::new(MetadataSourceMode::Cache),
            MetadataSourceCandidates {
                official: MetadataSourceCandidate::available(SelectedMetadataSource::official()),
                ..MetadataSourceCandidates::default()
            },
        )
        .expect_err("cache mode should not fall through to official");

        assert!(matches!(
            error,
            MetadataSourceResolutionError::Unavailable(message) if message.contains("cache")
        ));
    }

    #[test]
    fn metadata_source_auto_skips_catalog_when_gate_is_off() {
        let context = test_context(Some("false"));
        let selected = resolve_metadata_source_selection(
            MetadataSourceResolutionRequest::from_context(
                MetadataSourceMode::Auto,
                false,
                &context,
            ),
            MetadataSourceCandidates {
                catalog: MetadataSourceCandidate::available(SelectedMetadataSource::catalog()),
                official: MetadataSourceCandidate::available(SelectedMetadataSource::official()),
                ..MetadataSourceCandidates::default()
            },
        )
        .expect("official source should be selected when catalog is gated off");

        assert_eq!(selected.kind(), SelectedMetadataSourceKind::Official);
    }

    #[test]
    fn metadata_source_auto_falls_back_from_catalog_network_failure_to_official() {
        let context = test_context(Some("true"));
        let selected = resolve_metadata_source_selection(
            MetadataSourceResolutionRequest::from_context(
                MetadataSourceMode::Auto,
                false,
                &context,
            ),
            MetadataSourceCandidates {
                catalog: MetadataSourceCandidate::network_failure("temporary network outage"),
                official: MetadataSourceCandidate::available(SelectedMetadataSource::official()),
                ..MetadataSourceCandidates::default()
            },
        )
        .expect("official source should be selected after catalog network failure");

        assert_eq!(selected.kind(), SelectedMetadataSourceKind::Official);
    }

    #[test]
    fn metadata_source_auto_does_not_fallback_from_catalog_trust_failure() {
        let context = test_context(Some("on"));
        let error = resolve_metadata_source_selection(
            MetadataSourceResolutionRequest::from_context(
                MetadataSourceMode::Auto,
                false,
                &context,
            ),
            MetadataSourceCandidates {
                catalog: MetadataSourceCandidate::trust_failure(catalog_trust_failure()),
                official: MetadataSourceCandidate::available(SelectedMetadataSource::official()),
                ..MetadataSourceCandidates::default()
            },
        )
        .expect_err("catalog trust failure should stop source resolution");

        assert!(matches!(
            error,
            MetadataSourceResolutionError::Trust(CatalogTrustFailure::SignatureMismatch { .. })
        ));
    }

    #[test]
    fn metadata_source_official_mode_skips_catalog() {
        let selected = resolve_metadata_source_selection(
            MetadataSourceResolutionRequest::new(MetadataSourceMode::Official)
                .catalog_enabled(true),
            MetadataSourceCandidates {
                catalog: MetadataSourceCandidate::trust_failure(catalog_trust_failure()),
                official: MetadataSourceCandidate::available(SelectedMetadataSource::official()),
                ..MetadataSourceCandidates::default()
            },
        )
        .expect("official source should skip catalog candidates");

        assert_eq!(selected.kind(), SelectedMetadataSourceKind::Official);
    }

    #[test]
    fn metadata_source_selection_metadata_is_recorded() {
        let metadata = SelectedMetadataSource::catalog()
            .with_catalog_version("2026.05.22")
            .with_manifest_sha256("sha256:manifest")
            .with_payload_sha256("sha256:payload")
            .with_metadata("provider", "official")
            .validator_metadata();

        assert_eq!(
            metadata.get("source_kind").map(String::as_str),
            Some("catalog")
        );
        assert_eq!(
            metadata.get("catalog_version").map(String::as_str),
            Some("2026.05.22")
        );
        assert_eq!(
            metadata.get("manifest_sha256").map(String::as_str),
            Some("sha256:manifest")
        );
        assert_eq!(
            metadata.get("payload_sha256").map(String::as_str),
            Some("sha256:payload")
        );
        assert_eq!(
            metadata.get("provider").map(String::as_str),
            Some("official")
        );
    }

    #[test]
    fn catalog_diagnostics_unknown_trust_root_error_has_next_action() {
        let error = catalog_trust_failure_error(CatalogTrustFailure::UnknownTrustRoot {
            trust_root_id: "org-mirror".to_owned(),
        });
        let message = error.to_string();

        assert!(message.contains("catalog trust failure"));
        assert!(message.contains("unknown catalog trust root `org-mirror`"));
        assert!(message.contains("next:"));
        assert!(message.contains("do not ignore catalog trust failures"));
    }
}
