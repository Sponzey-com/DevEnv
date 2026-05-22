use std::collections::BTreeMap;
use std::path::Path;

use devenv_core::{
    ACTIVE_SHIM_ENV, ActivationPlan, CommandOutput, FakeCommandRunner, FakeShimWriter,
    FakeToolAdapter, ShimSpec, ToolName, collect_shim_specs, dispatch_shim_command, rehash_shims,
    tool_for_shim_binary,
};

#[test]
fn fake_adapter_exposed_binaries_generate_expected_shim_entries() {
    let adapter = FakeToolAdapter::new("fake").with_exposed_binaries(["fake", "fakefmt"]);
    let adapters = [&adapter as &dyn devenv_core::ToolAdapter];

    let specs = collect_shim_specs(&adapters).expect("shim specs should collect");

    assert_eq!(
        specs,
        vec![
            ShimSpec::new(ToolName::new("fake").expect("tool should be valid"), "fake"),
            ShimSpec::new(
                ToolName::new("fake").expect("tool should be valid"),
                "fakefmt"
            ),
        ]
    );
}

#[test]
fn rehash_writes_all_collected_shims_through_writer_port() {
    let adapter = FakeToolAdapter::new("fake").with_exposed_binaries(["fake", "fakefmt"]);
    let adapters = [&adapter as &dyn devenv_core::ToolAdapter];
    let mut writer = FakeShimWriter::default();

    let specs = rehash_shims(&adapters, &mut writer).expect("shims should rehash");

    assert_eq!(writer.written(), specs);
}

#[test]
fn exposed_binary_maps_back_to_owning_tool() {
    let fake = FakeToolAdapter::new("fake").with_exposed_binaries(["fake"]);
    let other = FakeToolAdapter::new("other").with_exposed_binaries(["other"]);
    let adapters = [
        &fake as &dyn devenv_core::ToolAdapter,
        &other as &dyn devenv_core::ToolAdapter,
    ];

    let tool = tool_for_shim_binary("other", &adapters)
        .expect("lookup should succeed")
        .expect("tool should resolve");

    assert_eq!(tool.as_str(), "other");
}

#[test]
fn duplicate_shim_binary_across_tools_is_rejected() {
    let left = FakeToolAdapter::new("left").with_exposed_binaries(["tool"]);
    let right = FakeToolAdapter::new("right").with_exposed_binaries(["tool"]);
    let adapters = [
        &left as &dyn devenv_core::ToolAdapter,
        &right as &dyn devenv_core::ToolAdapter,
    ];

    let error = collect_shim_specs(&adapters).expect_err("duplicate shim should fail");

    assert!(error.to_string().contains("tool"));
    assert!(error.to_string().contains("left"));
    assert!(error.to_string().contains("right"));
}

#[test]
fn shim_dispatch_calls_target_binary_through_command_runner() {
    let activation = ActivationPlan::new().prepend_path("/opt/fake/bin");
    let environment = BTreeMap::from([("PATH".to_owned(), "/devenv/shims:/usr/bin".to_owned())]);
    let mut runner = FakeCommandRunner::default().with_output(CommandOutput::new(0, "ok", ""));
    let args = vec!["--version".to_owned()];

    let output = dispatch_shim_command(
        "fake",
        &args,
        activation,
        Path::new("/workspace"),
        &environment,
        &mut runner,
    )
    .expect("shim dispatch should run");

    assert_eq!(output.stdout(), "ok");
    let invocation = runner
        .invocations()
        .first()
        .expect("runner should record invocation");
    assert_eq!(invocation.command(), "fake");
    assert_eq!(invocation.args(), &["--version"]);
    assert_eq!(invocation.cwd(), Some(Path::new("/workspace")));
    assert_eq!(
        invocation
            .env_delta()
            .sets()
            .get(ACTIVE_SHIM_ENV)
            .map(String::as_str),
        Some("fake")
    );
    assert_eq!(
        invocation
            .env_delta()
            .sets()
            .get("PATH")
            .map(String::as_str),
        Some("/opt/fake/bin:/devenv/shims:/usr/bin")
    );
}

#[test]
fn shim_recursion_is_detected_before_running_target() {
    let activation = ActivationPlan::new().prepend_path("/devenv/shims");
    let environment = BTreeMap::from([(ACTIVE_SHIM_ENV.to_owned(), "fake".to_owned())]);
    let mut runner = FakeCommandRunner::default();

    let error = dispatch_shim_command(
        "fake",
        &[],
        activation,
        Path::new("/workspace"),
        &environment,
        &mut runner,
    )
    .expect_err("recursion should fail");

    assert!(error.to_string().contains("shim recursion detected"));
    assert!(runner.invocations().is_empty());
}
