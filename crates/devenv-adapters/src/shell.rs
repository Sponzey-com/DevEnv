use devenv_core::{ActivationPlan, ActivationRenderer, CoreResult, EnvOperation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShellSyntax {
    Posix,
    Bash,
    Zsh,
    Fish,
    PowerShell,
}

#[derive(Debug, Clone)]
pub struct ShellActivationRenderer {
    syntax: ShellSyntax,
}

impl ShellActivationRenderer {
    pub fn new(syntax: ShellSyntax) -> Self {
        Self { syntax }
    }
}

impl Default for ShellActivationRenderer {
    fn default() -> Self {
        Self::new(ShellSyntax::Posix)
    }
}

impl ActivationRenderer for ShellActivationRenderer {
    fn render(&self, plan: &ActivationPlan) -> CoreResult<String> {
        match self.syntax {
            ShellSyntax::Posix | ShellSyntax::Bash | ShellSyntax::Zsh => Ok(render_posix(plan)),
            ShellSyntax::Fish => Ok(render_fish(plan)),
            ShellSyntax::PowerShell => Ok(render_powershell(plan)),
        }
    }
}

fn render_posix(plan: &ActivationPlan) -> String {
    plan.operations()
        .iter()
        .map(|operation| match operation {
            EnvOperation::Set { key, value } => {
                format!("export {key}={}", quote_posix(value))
            }
            EnvOperation::Unset { key } => format!("unset {key}"),
            EnvOperation::PrependPath { path } => {
                format!(
                    "export PATH={}:\"$PATH\"",
                    quote_posix(&path.to_string_lossy())
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn render_fish(plan: &ActivationPlan) -> String {
    plan.operations()
        .iter()
        .map(|operation| match operation {
            EnvOperation::Set { key, value } => {
                format!("set -gx {key} {}", quote_fish(value))
            }
            EnvOperation::Unset { key } => format!("set -e {key}"),
            EnvOperation::PrependPath { path } => {
                format!("set -gx PATH {} $PATH", quote_fish(&path.to_string_lossy()))
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quote_fish(value: &str) -> String {
    quote_posix(value)
}

fn render_powershell(plan: &ActivationPlan) -> String {
    plan.operations()
        .iter()
        .map(|operation| match operation {
            EnvOperation::Set { key, value } => {
                format!("$env:{key} = {}", quote_powershell(value))
            }
            EnvOperation::Unset { key } => {
                format!("Remove-Item Env:{key} -ErrorAction SilentlyContinue")
            }
            EnvOperation::PrependPath { path } => {
                format!(
                    "$env:PATH = {} + [IO.Path]::PathSeparator + $env:PATH",
                    quote_powershell(&path.to_string_lossy())
                )
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quote_powershell(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
