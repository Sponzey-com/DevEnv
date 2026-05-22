use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use devenv_adapters::archive::ManifestArchiveExtractor;
use devenv_adapters::checksum::Sha256ChecksumVerifier;
use devenv_adapters::download::FileDownloader;
use devenv_adapters::fs::{
    NativeConfigRepository, discover_project_config_from, read_devenv_toml_config,
};
use devenv_adapters::install::{FileInstallTransactionManager, SystemClock};
use devenv_adapters::process::ProcessCommandRunner;
use devenv_adapters::shell::{ShellActivationRenderer, ShellSyntax};
use devenv_adapters::shim::FileShimWriter;
use devenv_adapters::store::{DevEnvHome, FileInstallStore, FileRuntimeRegistry};
use devenv_core::{
    ActivationPlan, ActivationRenderer, Architecture, ConfigRepository, ConfigScope, CoreError,
    ExecCommand, InMemoryLockManager, InstallRuntimePorts, InstallRuntimeRequest, OperatingSystem,
    Platform, ProjectConfig, RegisteredRuntime, SelectionCandidate, SelectionSource, ToolAdapter,
    ToolName, ToolSpec, Version, VersionRequirement, VersionSource,
    activation_plan_for_selected_runtime, add_external_runtime, collect_shim_specs,
    dispatch_shim_command, install_runtime, list_remote_versions, rehash_shims,
    remove_external_runtime, resolve_tool_selection, tool_for_shim_binary, uninstall_runtime,
};
use devenv_tools::{
    FlutterArtifactResolver, FlutterInstalledRuntimeValidator, FlutterReleaseMetadata,
    FlutterReleaseVersionSource, FlutterRuntime, FlutterRuntimeDiscovery, FlutterRuntimeSource,
    FlutterToolAdapter, FlutterVersionMatcher, GoArtifactResolver, GoInstalledRuntimeValidator,
    GoReleaseMetadata, GoReleaseVersionSource, GoRuntime, GoRuntimeDiscovery, GoRuntimeSource,
    GoToolAdapter, GoVersionMatcher, IacArtifactResolver, IacReleaseMetadata,
    IacReleaseVersionSource, IacRuntime, IacRuntimeDiscovery, IacRuntimeSource, IacTool,
    IacVersionMatcher, JavaArtifactResolver, JavaInstalledRuntimeValidator, JavaReleaseMetadata,
    JavaReleaseVersionSource, JavaRuntime, JavaRuntimeDiscovery, JavaRuntimeSource,
    JavaToolAdapter, JavaVersionMatcher, NodeArtifactResolver, NodeInstalledRuntimeValidator,
    NodeReleaseMetadata, NodeReleaseVersionSource, NodeRuntime, NodeRuntimeDiscovery,
    NodeRuntimeSource, NodeToolAdapter, NodeVersionMatcher, OpenTofuInstalledRuntimeValidator,
    OpenTofuToolAdapter, PhpRuntime, PhpRuntimeDiscovery, PhpRuntimeSource, PhpToolAdapter,
    PhpVersionMatcher, PythonArtifactResolver, PythonInstalledRuntimeValidator,
    PythonReleaseMetadata, PythonReleaseVersionSource, PythonRuntime, PythonRuntimeDiscovery,
    PythonRuntimeSource, PythonToolAdapter, PythonVersionMatcher, RubyRuntime,
    RubyRuntimeDiscovery, RubyRuntimeSource, RubyToolAdapter, RubyVersionMatcher, RustRuntime,
    RustRuntimeDiscovery, RustRuntimeSource, RustToolAdapter, RustVersionMatcher,
    TerraformInstalledRuntimeValidator, TerraformToolAdapter, builtin_tool_adapter,
    match_flutter_runtime, match_go_runtime, match_iac_runtime, match_java_runtime,
    match_node_runtime, match_php_runtime, match_python_runtime, match_ruby_runtime,
    match_rust_runtime, normalize_flutter_version, normalize_go_version, normalize_iac_version,
    normalize_node_version, normalize_php_version, normalize_python_version,
    normalize_ruby_version, normalize_rust_version, validate_flutter_sdk_home,
    validate_go_sdk_home, validate_iac_tool_home, validate_jdk_home, validate_node_home,
    validate_php_home, validate_python_home, validate_ruby_home, validate_rust_toolchain_home,
};

pub const COMMAND_NAME: &str = "devenv";
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
const FLUTTER_RELEASE_METADATA_ENV: &str = "DEVENV_FLUTTER_RELEASE_METADATA";
const TERRAFORM_RELEASE_METADATA_ENV: &str = "DEVENV_TERRAFORM_RELEASE_METADATA";
const OPENTOFU_RELEASE_METADATA_ENV: &str = "DEVENV_OPENTOFU_RELEASE_METADATA";
const JAVA_RELEASE_METADATA_ENV: &str = "DEVENV_JAVA_RELEASE_METADATA";
const NODE_RELEASE_METADATA_ENV: &str = "DEVENV_NODE_RELEASE_METADATA";
const PYTHON_RELEASE_METADATA_ENV: &str = "DEVENV_PYTHON_RELEASE_METADATA";

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
  exec          Run a command with selected tool environments.
  activate      Print shell activation for DevEnv shims.
  shim          Manage and dispatch shims.
  doctor        Check DevEnv home, registry, installs, and shims.
  help          Show this help or command-specific help.

Supported tools:
  java, go, node, python, ruby, php, rust, flutter, terraform, opentofu

Examples:
  devenv add java /path/to/jdk
  devenv local java@17
  devenv install go@1.22
  devenv uninstall go@1.22
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
       devenv remove <tool>@<version> [path]

Removes external runtime registrations. Runtime files are not deleted.

Example:
  devenv remove go /usr/local/go"#,
        ),
        "install" => Some(
            r#"Usage: devenv install <tool>@<version>

Installs a runtime into DevEnv-owned storage. Remote metadata must be configured for installable tools.

Example:
  devenv install go@1.22"#,
        ),
        "uninstall" => Some(
            r#"Usage: devenv uninstall <tool>@<version>

Deletes only DevEnv-owned installs for the current platform. External registrations created with `devenv add` are not deleted.

Example:
  devenv uninstall go@1.22"#,
        ),
        "local" => Some(
            r#"Usage: devenv local <tool>@<version> [--dry-run]

Writes a project-local selection to devenv.toml in the current directory.

Example:
  devenv local java@17"#,
        ),
        "global" => Some(
            r#"Usage: devenv global <tool>@<version> [--dry-run]

Writes a global selection to the file pointed to by DEVENV_GLOBAL_CONFIG.

Example:
  devenv global node@20"#,
        ),
        "shell" => Some(
            r#"Usage: devenv shell <tool>@<version> [--dry-run]

Prints shell-scoped environment exports for one selected tool.

Example:
  eval "$(devenv shell python@3.12)""#,
        ),
        "use" => Some(
            r#"Usage: devenv use <tool>@<version> [--scope local|global|shell] [--dry-run]

Selects a tool version. The default scope is local.

Example:
  devenv use ruby@3.3 --scope local"#,
        ),
        "current" => Some(
            r#"Usage: devenv current [<tool>|<tool>@<version>]

Shows selected versions after applying CLI, shell, project, and global precedence.

Example:
  devenv current
  devenv current java"#,
        ),
        "list" => Some(
            r#"Usage: devenv list <tool>

Lists DevEnv-owned installs, external registrations, and configured candidate runtimes.

Example:
  devenv list go"#,
        ),
        "list-remote" => Some(
            r#"Usage: devenv list-remote <tool>

Lists installable versions from configured release metadata.

Example:
  devenv list-remote python"#,
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

Example:
  eval "$(devenv activate zsh)""#,
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

    let (spec, dry_run) = parse_scoped_write_args(args)?;
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

    if args.len() > 1 {
        return Err(CliError::usage(
            "usage: devenv current [<tool>|<tool>@<version>]".to_owned(),
        ));
    }

    let project_config = discover_project_config_from(&context.current_dir)?;
    let global_config = context
        .global_config_path
        .as_ref()
        .map(|path| read_devenv_toml_config(path, ConfigScope::Global))
        .transpose()?
        .flatten();

    match args.first() {
        Some(selector) if selector.contains('@') => {
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
        Some(selector) => {
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
        None => {
            let tools = collect_configured_tools(
                project_config.as_ref(),
                global_config.as_ref(),
                context.env_vars.iter(),
            );
            if tools.is_empty() {
                return Err(CliError::runtime(
                    "no versions selected. Run `devenv local java@17`, `devenv global go@1.22.5`, or `eval \"$(devenv shell java@17)\"`.",
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
                    "no versions selected. Run `devenv local java@17`, `devenv global go@1.22.5`, or `eval \"$(devenv shell java@17)\"`.",
                ))
            }
        }
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
    let global_config = context
        .global_config_path
        .as_ref()
        .map(|path| read_devenv_toml_config(path, ConfigScope::Global))
        .transpose()?
        .flatten();
    let selections =
        resolve_all_selected_tools(project_config.as_ref(), global_config.as_ref(), context)?;

    if selections.is_empty() {
        return Err(CliError::runtime(
            "no versions selected. Run `devenv local java@17`, `devenv global go@1.22.5`, or `eval \"$(devenv shell java@17)\"` before `devenv exec -- <command>`.",
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
    let mut writer = FileShimWriter::new(&shim_dir);
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
    let global_config = context
        .global_config_path
        .as_ref()
        .map(|path| read_devenv_toml_config(path, ConfigScope::Global))
        .transpose()?
        .flatten();
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
            "usage: devenv remove <tool> <path>\n       devenv remove <tool>@<version> [path]"
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

    if args.len() != 1 {
        return Err(CliError::usage(
            "usage: devenv list-remote <tool>".to_owned(),
        ));
    }

    let tool = ToolName::new(&args[0]).map_err(CoreError::from)?;
    match tool.as_str() {
        "java" => {
            let source = JavaReleaseVersionSource::new(load_java_release_metadata(context)?);
            write_remote_versions(&tool, &source, Some(source.distribution().as_str()), stdout)
        }
        "go" => {
            let source = GoReleaseVersionSource::new(load_go_release_metadata(context)?);
            write_remote_versions(&tool, &source, None, stdout)
        }
        "flutter" => {
            let source = FlutterReleaseVersionSource::new(load_flutter_release_metadata(context)?);
            write_remote_versions(&tool, &source, Some("stable"), stdout)
        }
        "terraform" => {
            let source = IacReleaseVersionSource::new(
                IacTool::Terraform,
                load_iac_release_metadata(IacTool::Terraform, context)?,
            );
            write_remote_versions(&tool, &source, None, stdout)
        }
        "opentofu" => {
            let source = IacReleaseVersionSource::new(
                IacTool::OpenTofu,
                load_iac_release_metadata(IacTool::OpenTofu, context)?,
            );
            write_remote_versions(&tool, &source, None, stdout)
        }
        "node" => {
            let source = NodeReleaseVersionSource::new(load_node_release_metadata(context)?);
            write_remote_versions(&tool, &source, None, stdout)
        }
        "python" => {
            let source = PythonReleaseVersionSource::new(load_python_release_metadata(context)?);
            write_remote_versions(&tool, &source, Some("cpython"), stdout)
        }
        _ => Err(CliError::runtime(format!(
            "`devenv list-remote` is not implemented for `{tool}` yet"
        ))),
    }
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

    if args.len() != 1 {
        return Err(CliError::usage(
            "usage: devenv install <tool>@<version>".to_owned(),
        ));
    }

    let spec = parse_tool_spec(&args[0])?;
    match spec.tool().as_str() {
        "java" => run_install_java(spec.requirement(), stdout, context),
        "go" => run_install_go(spec.requirement(), stdout, context),
        "flutter" => run_install_flutter(spec.requirement(), stdout, context),
        "terraform" => run_install_iac(IacTool::Terraform, spec.requirement(), stdout, context),
        "opentofu" => run_install_iac(IacTool::OpenTofu, spec.requirement(), stdout, context),
        "node" => run_install_node(spec.requirement(), stdout, context),
        "python" => run_install_python(spec.requirement(), stdout, context),
        _ => Err(CliError::runtime(format!(
            "`devenv install` is not implemented for `{}` yet",
            spec.tool()
        ))),
    }
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

    if args.len() != 1 {
        return Err(CliError::usage(
            "usage: devenv uninstall <tool>@<version>".to_owned(),
        ));
    }

    let spec = parse_tool_spec(&args[0])?;
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
    let plan = ActivationPlan::new()
        .set_env("DEVENV_HOME", home.root().to_string_lossy())
        .prepend_path(home.shims_dir());
    let renderer = ShellActivationRenderer::new(syntax);

    writeln!(stdout, "{}", renderer.render(&plan)?)?;
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

fn run_install_go<O>(
    requirement: &VersionRequirement,
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let version =
        Version::new(normalize_go_version(requirement.raw())?).map_err(CoreError::from)?;
    let metadata = load_go_release_metadata(context)?;
    let resolver = GoArtifactResolver::new(metadata);
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;

    let mut downloader = FileDownloader;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = GoInstalledRuntimeValidator;
    let metadata = install_runtime(
        install_request_with_resolution_metadata(
            ToolName::new("go").map_err(CoreError::from)?,
            requirement,
            version,
        ),
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
            "failed to install go@{}: {error}\nnext: verify `{GO_RELEASE_METADATA_ENV}` points to readable metadata with a local file or file:// artifact URL and a matching sha256 checksum.",
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
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let requested =
        Version::new(normalize_flutter_version(requirement.raw())?).map_err(CoreError::from)?;
    let release_metadata = load_flutter_release_metadata(context)?;
    let resolver = FlutterArtifactResolver::new(release_metadata);
    let install_version = resolver.resolve_install_version(&requested)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;

    let mut downloader = FileDownloader;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = FlutterInstalledRuntimeValidator;
    let metadata = install_runtime(
        install_request_with_resolution_metadata(
            ToolName::new("flutter").map_err(CoreError::from)?,
            requirement,
            install_version,
        ),
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
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let requested =
        Version::new(normalize_iac_version(requirement.raw())?).map_err(CoreError::from)?;
    let release_metadata = load_iac_release_metadata(tool, context)?;
    let resolver = IacArtifactResolver::new(tool, release_metadata);
    let install_version = resolver.resolve_install_version(&requested)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;

    let mut downloader = FileDownloader;
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
        install_request_with_resolution_metadata(tool.tool_name(), requirement, install_version),
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
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let requested = Version::new(requirement.raw()).map_err(CoreError::from)?;
    let release_metadata = load_java_release_metadata(context)?;
    let resolver = JavaArtifactResolver::new(release_metadata);
    let install_version = resolver.resolve_install_version(&requested)?;
    let distribution = resolver.distribution().as_str().to_owned();
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;

    let mut downloader = FileDownloader;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = JavaInstalledRuntimeValidator;
    let metadata = install_runtime(
        install_request_with_resolution_metadata(
            ToolName::new("java").map_err(CoreError::from)?,
            requirement,
            install_version,
        )
        .with_metadata_field("distribution", distribution.clone()),
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
    stdout: &mut O,
    context: &CommandContext,
) -> Result<(), CliError>
where
    O: Write,
{
    let requested = Version::new(requirement.raw()).map_err(CoreError::from)?;
    let release_metadata = load_node_release_metadata(context)?;
    let resolver = NodeArtifactResolver::new(release_metadata);
    let install_version = resolver.resolve_install_version(&requested)?;
    let home = DevEnvHome::resolve_from_env(&context.env_vars)?;
    home.create_layout()?;

    let mut downloader = FileDownloader;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = NodeInstalledRuntimeValidator;
    let metadata = install_runtime(
        install_request_with_resolution_metadata(
            ToolName::new("node").map_err(CoreError::from)?,
            requirement,
            install_version,
        ),
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
    home.create_layout()?;

    let mut downloader = FileDownloader;
    let checksum = Sha256ChecksumVerifier;
    let mut extractor = ManifestArchiveExtractor;
    let mut transactions = FileInstallTransactionManager::at_home(&home);
    let mut install_store = FileInstallStore::at_home(&home);
    let mut lock_manager = InMemoryLockManager::default();
    let clock = SystemClock;
    let validator = PythonInstalledRuntimeValidator;
    let metadata = install_runtime(
        install_request_with_resolution_metadata(
            ToolName::new("python").map_err(CoreError::from)?,
            requirement,
            install_version,
        )
        .with_metadata_field("implementation", "cpython"),
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
            }
            write_selection_change(
                stdout,
                "local",
                spec.tool(),
                spec.requirement(),
                Some(&path),
            )
        }
        ScopeCommand::Global => {
            let path = context.global_config_path.as_ref().ok_or_else(|| {
                CliError::runtime(format!(
                    "global config path is not configured. Set `{GLOBAL_CONFIG_ENV}` to the devenv.toml path to update."
                ))
            })?;
            if !dry_run {
                let mut repository = NativeConfigRepository::new(path, ConfigScope::Global);
                repository.set_requirement(spec.tool().clone(), spec.requirement().clone())?;
            }
            write_selection_change(
                stdout,
                "global",
                spec.tool(),
                spec.requirement(),
                Some(path),
            )
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

fn load_go_release_metadata(context: &CommandContext) -> Result<GoReleaseMetadata, CliError> {
    load_release_metadata(
        context,
        ReleaseMetadataInput::fixture("Go", GO_RELEASE_METADATA_ENV),
        GoReleaseMetadata::parse,
    )
}

fn load_flutter_release_metadata(
    context: &CommandContext,
) -> Result<FlutterReleaseMetadata, CliError> {
    load_release_metadata(
        context,
        ReleaseMetadataInput::fixture("Flutter", FLUTTER_RELEASE_METADATA_ENV),
        FlutterReleaseMetadata::parse,
    )
}

fn load_iac_release_metadata(
    tool: IacTool,
    context: &CommandContext,
) -> Result<IacReleaseMetadata, CliError> {
    load_release_metadata(
        context,
        ReleaseMetadataInput::fixture(tool.display_name(), iac_release_metadata_env(tool)),
        IacReleaseMetadata::parse,
    )
}

fn iac_release_metadata_env(tool: IacTool) -> &'static str {
    match tool {
        IacTool::Terraform => TERRAFORM_RELEASE_METADATA_ENV,
        IacTool::OpenTofu => OPENTOFU_RELEASE_METADATA_ENV,
    }
}

fn load_java_release_metadata(context: &CommandContext) -> Result<JavaReleaseMetadata, CliError> {
    load_release_metadata(
        context,
        ReleaseMetadataInput::fixture("Java", JAVA_RELEASE_METADATA_ENV),
        JavaReleaseMetadata::parse,
    )
}

fn load_node_release_metadata(context: &CommandContext) -> Result<NodeReleaseMetadata, CliError> {
    load_release_metadata(
        context,
        ReleaseMetadataInput::fixture("Node.js", NODE_RELEASE_METADATA_ENV),
        NodeReleaseMetadata::parse,
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
) -> Result<String, CliError> {
    let path = resolve_input_path(path, &context.current_dir);
    std::fs::read_to_string(&path).map_err(|error| {
        CliError::runtime(format!(
            "failed to read {} release metadata `{}`: {error}",
            input.display_name,
            path.display()
        ))
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

fn parse_scoped_write_args(args: &[String]) -> Result<(ToolSpec, bool), CliError> {
    let mut spec = None;
    let mut dry_run = false;

    for arg in args {
        match arg.as_str() {
            "--dry-run" => dry_run = true,
            value if spec.is_none() => spec = Some(parse_tool_spec(value)?),
            value => {
                return Err(CliError::usage(format!(
                    "unexpected argument `{value}`\nusage: devenv local <tool>@<version>"
                )));
            }
        }
    }

    let spec = spec.ok_or_else(|| {
        CliError::usage("missing tool spec\nusage: devenv local <tool>@<version>".to_owned())
    })?;

    Ok((spec, dry_run))
}

fn parse_use_args(args: &[String]) -> Result<(ToolSpec, ScopeCommand, bool), CliError> {
    let mut spec = None;
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
            value if spec.is_none() => {
                spec = Some(parse_tool_spec(value)?);
                index += 1;
            }
            value => {
                return Err(CliError::usage(format!(
                    "unexpected argument `{value}`\nusage: devenv use <tool>@<version> [--scope local|global|shell]"
                )));
            }
        }
    }

    let spec = spec.ok_or_else(|| {
        CliError::usage(
            "missing tool spec\nusage: devenv use <tool>@<version> [--scope local|global|shell]"
                .to_owned(),
        )
    })?;

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

fn missing_selection_error(tool: &ToolName) -> CliError {
    CliError::runtime(format!(
        "no version selected for {tool}. Run `devenv local {tool}@<version>`, `devenv global {tool}@<version>`, or `eval \"$(devenv shell {tool}@<version>)\"`."
    ))
}

fn missing_runtime_error(tool: &ToolName, requirement: &VersionRequirement) -> CliError {
    CliError::runtime(format!(
        "{}@{} is selected but not installed or registered.\nRun `devenv add {} <path>` for an existing runtime, `devenv install {}@{}` for a DevEnv-owned runtime, or `devenv list {}` to inspect known runtimes.",
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
                Some("run `devenv install <tool>@<version>` or `devenv shim init`"),
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
                Some("run `devenv local <tool>@<version>` to create one"),
            )),
            Err(error) => checks.push(DoctorCheck::error(
                "project config",
                error.to_string(),
                Some(context.current_dir.clone()),
                Some("fix the nearest devenv.toml file"),
            )),
        }

        if let Some(path) = &context.global_config_path {
            match read_devenv_toml_config(path, ConfigScope::Global) {
                Ok(Some(_)) => checks.push(DoctorCheck::ok(
                    "global config",
                    "global config is readable",
                    Some(path.clone()),
                    None,
                )),
                Ok(None) => checks.push(DoctorCheck::warning(
                    "global config",
                    "global config path is configured but file does not exist",
                    Some(path.clone()),
                    Some("run `devenv global <tool>@<version>`"),
                )),
                Err(error) => checks.push(DoctorCheck::error(
                    "global config",
                    error.to_string(),
                    Some(path.clone()),
                    Some("fix or remove DEVENV_GLOBAL_CONFIG"),
                )),
            }
        } else {
            checks.push(DoctorCheck::ok(
                "global config",
                "DEVENV_GLOBAL_CONFIG is not set",
                None,
                Some("set DEVENV_GLOBAL_CONFIG to enable global selections"),
            ));
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
        Self::Runtime(value.to_string())
    }
}

impl From<io::Error> for CliError {
    fn from(value: io::Error) -> Self {
        Self::Runtime(value.to_string())
    }
}
