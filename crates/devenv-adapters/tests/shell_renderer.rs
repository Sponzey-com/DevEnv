use std::path::PathBuf;

use devenv_adapters::shell::{ShellActivationRenderer, ShellSyntax};
use devenv_core::{ActivationPlan, ActivationRenderer};

#[test]
fn posix_shell_renderer_renders_activation_operations() {
    let renderer = ShellActivationRenderer::default();
    let plan = ActivationPlan::new()
        .set_env("JAVA_HOME", "/opt/jdk")
        .prepend_path(PathBuf::from("/opt/jdk/bin"))
        .unset_env("OLD_JAVA_HOME");

    let rendered = renderer.render(&plan).expect("plan should render");

    assert!(rendered.contains("export JAVA_HOME='/opt/jdk'"));
    assert!(rendered.contains("export PATH='/opt/jdk/bin':\"$PATH\""));
    assert!(rendered.contains("unset OLD_JAVA_HOME"));
}

#[test]
fn shell_renderers_produce_stable_activation_output() {
    let plan = ActivationPlan::new()
        .set_env("DEVENV_HOME", "/tmp/devenv")
        .prepend_path(PathBuf::from("/tmp/devenv/shims"));

    let bash = ShellActivationRenderer::new(ShellSyntax::Bash)
        .render(&plan)
        .expect("bash should render");
    assert_eq!(
        bash,
        "export DEVENV_HOME='/tmp/devenv'\nexport PATH='/tmp/devenv/shims':\"$PATH\""
    );

    let zsh = ShellActivationRenderer::new(ShellSyntax::Zsh)
        .render(&plan)
        .expect("zsh should render");
    assert_eq!(zsh, bash);

    let fish = ShellActivationRenderer::new(ShellSyntax::Fish)
        .render(&plan)
        .expect("fish should render");
    assert_eq!(
        fish,
        "set -gx DEVENV_HOME '/tmp/devenv'\nset -gx PATH '/tmp/devenv/shims' $PATH"
    );

    let powershell = ShellActivationRenderer::new(ShellSyntax::PowerShell)
        .render(&plan)
        .expect("powershell should render");
    assert_eq!(
        powershell,
        "$env:DEVENV_HOME = '/tmp/devenv'\n$env:PATH = '/tmp/devenv/shims' + [IO.Path]::PathSeparator + $env:PATH"
    );
}
