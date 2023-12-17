use std::{
    io,
    process::{Child, Command, Stdio},
};

/// Unix-specific state required to run processes transparently.
#[derive(Clone, Debug, Default)]
pub struct TransparentRunnerImpl;

impl TransparentRunnerImpl {
    pub fn spawn_transparent(&self, command: &Command) -> u32 {
        let mut runner_command = Command::new("xvfb-run");
        runner_command
            .arg("--auto-servernum")
            .arg(command.get_program())
            .args(command.get_args())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        for env in command.get_envs() {
            match env {
                (k, Some(v)) => runner_command.env(k, v),
                (k, None) => runner_command.env_remove(k),
            };
        }

        if let Some(cd) = command.get_current_dir() {
            runner_command.current_dir(cd);
        } else {
            runner_command.current_dir(std::env::current_dir()?);
        }

        runner_command.spawn().unwrap_or_else(|
            e| panic!("Failed to spawn xvfb-run: {}", e)
        ).id() as u32

    }
}
