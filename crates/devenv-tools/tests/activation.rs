use std::path::PathBuf;

use devenv_core::{EnvOperation, ToolAdapter};
use devenv_tools::{
    FlutterToolAdapter, GoToolAdapter, JavaToolAdapter, OpenTofuToolAdapter, PhpToolAdapter,
    PythonToolAdapter, RubyToolAdapter, RustToolAdapter, TerraformToolAdapter,
};

#[test]
fn java_activation_sets_java_home_and_prepends_bin() {
    let adapter = JavaToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/jdk-17").as_path())
        .expect("java activation should be built");

    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::Set { key, value }
            if key == "JAVA_HOME" && value == "/opt/jdk-17"
    ));
    assert!(matches!(
        &plan.operations()[1],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/jdk-17/bin")
    ));
}

#[test]
fn go_activation_sets_goroot_and_prepends_bin() {
    let adapter = GoToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/go-1.22.5").as_path())
        .expect("go activation should be built");

    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::Set { key, value }
            if key == "GOROOT" && value == "/opt/go-1.22.5"
    ));
    assert!(matches!(
        &plan.operations()[1],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/go-1.22.5/bin")
    ));
}

#[test]
fn flutter_activation_sets_flutter_root_and_prepends_bin() {
    let adapter = FlutterToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/flutter-3.24.0").as_path())
        .expect("flutter activation should be built");

    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::Set { key, value }
            if key == "FLUTTER_ROOT" && value == "/opt/flutter-3.24.0"
    ));
    assert!(matches!(
        &plan.operations()[1],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/flutter-3.24.0/bin")
    ));
}

#[test]
fn terraform_activation_prepends_single_binary_root() {
    let adapter = TerraformToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/terraform-1.8.5").as_path())
        .expect("terraform activation should be built");

    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/terraform-1.8.5")
    ));
}

#[test]
fn ruby_activation_prepends_runtime_bin() {
    let adapter = RubyToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/ruby-3.3.0").as_path())
        .expect("ruby activation should be built");

    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/ruby-3.3.0/bin")
    ));
}

#[test]
fn php_activation_prepends_runtime_bin() {
    let adapter = PhpToolAdapter::new();
    let plan = adapter
        .activation_plan(PathBuf::from("/opt/php-8.3.7").as_path())
        .expect("php activation should be built");

    assert!(matches!(
        &plan.operations()[0],
        EnvOperation::PrependPath { path }
            if path == &PathBuf::from("/opt/php-8.3.7/bin")
    ));
}

#[test]
fn java_exposes_expected_shim_binaries() {
    let adapter = JavaToolAdapter::new();

    assert_eq!(
        adapter.exposed_binaries(),
        ["java", "javac", "jar", "javadoc"]
    );
}

#[test]
fn go_exposes_expected_shim_binaries() {
    let adapter = GoToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["go", "gofmt"]);
}

#[test]
fn flutter_exposes_expected_shim_binaries() {
    let adapter = FlutterToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["flutter", "dart"]);
}

#[test]
fn terraform_exposes_expected_shim_binaries() {
    let adapter = TerraformToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["terraform"]);
}

#[test]
fn opentofu_exposes_expected_shim_binaries() {
    let adapter = OpenTofuToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["tofu"]);
}

#[test]
fn ruby_exposes_expected_shim_binaries() {
    let adapter = RubyToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["ruby", "gem", "bundle"]);
}

#[test]
fn php_exposes_expected_shim_binaries() {
    let adapter = PhpToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["php", "phpize", "php-config"]);
}

#[test]
fn python_exposes_expected_shim_binaries() {
    let adapter = PythonToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["python", "python3", "pip"]);
}

#[test]
fn rust_exposes_expected_shim_binaries() {
    let adapter = RustToolAdapter::new();

    assert_eq!(adapter.exposed_binaries(), ["rustc", "cargo"]);
}
