use std::path::PathBuf;

use devenv_catalog::{GenerateOptions, generate_catalog, verify_catalog};

fn main() {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help" || arg == "-h") {
        print_help();
        return Ok(());
    }

    match args[0].as_str() {
        "generate" => run_generate(&args[1..]),
        "verify" => run_verify(&args[1..]),
        command => Err(format!("unknown command `{command}`\n\n{}", help_text())),
    }
}

fn run_generate(args: &[String]) -> Result<(), String> {
    let mut source = None;
    let mut output = None;
    let mut generated_at = None;
    let mut expires_at = None;
    let mut catalog_version = None;
    let mut sequence = 1_u64;
    let mut min_devenv_version = "0.1.0".to_owned();
    let mut overrides = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--source" => {
                source = Some(take_value(args, index, "--source")?);
                index += 2;
            }
            "--output" => {
                output = Some(take_value(args, index, "--output")?);
                index += 2;
            }
            "--generated-at" => {
                generated_at = Some(take_value(args, index, "--generated-at")?);
                index += 2;
            }
            "--expires-at" => {
                expires_at = Some(take_value(args, index, "--expires-at")?);
                index += 2;
            }
            "--catalog-version" => {
                catalog_version = Some(take_value(args, index, "--catalog-version")?);
                index += 2;
            }
            "--sequence" => {
                sequence = take_value(args, index, "--sequence")?
                    .parse::<u64>()
                    .map_err(|error| format!("invalid --sequence value: {error}"))?;
                index += 2;
            }
            "--min-devenv-version" => {
                min_devenv_version = take_value(args, index, "--min-devenv-version")?;
                index += 2;
            }
            "--overrides" => {
                overrides = Some(take_value(args, index, "--overrides")?);
                index += 2;
            }
            value => {
                return Err(format!(
                    "unknown generate argument `{value}`\n\n{}",
                    help_text()
                ));
            }
        }
    }

    let mut options = GenerateOptions::new(
        required(source, "--source")?,
        required(output, "--output")?,
        required(generated_at, "--generated-at")?,
        required(expires_at, "--expires-at")?,
        required(catalog_version, "--catalog-version")?,
    )
    .with_sequence(sequence)
    .with_min_devenv_version(min_devenv_version);
    if let Some(overrides) = overrides {
        options = options.with_overrides_path(overrides);
    }

    let summary = generate_catalog(&options).map_err(|error| error.to_string())?;
    println!(
        "generated catalog {} entries={}",
        summary.manifest_path.display(),
        summary.entries
    );
    Ok(())
}

fn run_verify(args: &[String]) -> Result<(), String> {
    let mut catalog = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--catalog" => {
                catalog = Some(take_value(args, index, "--catalog")?);
                index += 2;
            }
            value => {
                return Err(format!(
                    "unknown verify argument `{value}`\n\n{}",
                    help_text()
                ));
            }
        }
    }

    let summary = verify_catalog(PathBuf::from(required(catalog, "--catalog")?))
        .map_err(|error| format!("catalog verification failed: {error}"))?;
    println!("verified catalog entries={}", summary.entries);
    Ok(())
}

fn take_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index + 1)
        .filter(|value| !value.starts_with("--"))
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn required(value: Option<String>, flag: &str) -> Result<String, String> {
    value.ok_or_else(|| format!("{flag} is required\n\n{}", help_text()))
}

fn print_help() {
    println!("{}", help_text());
}

fn help_text() -> &'static str {
    r#"Usage:
  devenv-catalog generate --source <dir> --output <dir> --generated-at <rfc3339> --expires-at <rfc3339> --catalog-version <version> [--sequence <n>] [--min-devenv-version <version>] [--overrides <file>]
  devenv-catalog verify --catalog <dir>

Commands:
  generate    Convert upstream Go/Node official metadata into normalized catalog payloads and manifest.
  verify      Verify manifest signature shim, manifest entry sha256 values, and payload tool/provider matches.

Source layout:
  go/official/releases.json
  node/official/index.json
  node/official/shasums/v<version>/SHASUMS256.txt

Override TOML:
  [[release]]
  tool = "go"
  version = "1.21.0"
  yanked = true
  deprecated = false
  reason = "manual catalog policy note""#
}
