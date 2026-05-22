use devenv_core::{CommandInvocation, CommandOutput, CommandRunner, CoreError, CoreResult};

#[derive(Debug, Clone, Default)]
pub struct ProcessCommandRunner;

impl CommandRunner for ProcessCommandRunner {
    fn run(&mut self, invocation: CommandInvocation) -> CoreResult<CommandOutput> {
        let mut command = std::process::Command::new(invocation.command());
        command.args(invocation.args());

        if let Some(cwd) = invocation.cwd() {
            command.current_dir(cwd);
        }

        for key in invocation.env_delta().unsets() {
            command.env_remove(key);
        }

        for (key, value) in invocation.env_delta().sets() {
            command.env(key, value);
        }

        let output = command.output().map_err(|error| {
            CoreError::message(format!(
                "failed to execute command `{}`: {error}",
                invocation.command()
            ))
        })?;

        Ok(CommandOutput::new(
            output.status.code().unwrap_or(1),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        ))
    }
}
