#[cfg(target_os = "windows")]
use crate::windows::virtual_desktop_runner::VirtualDesktop;
use std::{ffi::OsStr, path::PathBuf};
/// Windows-specific state required to run processes transparently
#[derive(Clone, Debug, Default)]
pub struct TransparentRunnerImpl {}
impl TransparentRunnerImpl {
    fn default() -> Self {
        TransparentRunnerImpl {}
    }

    pub fn spawn_transparent(&self, command: &std::process::Command) -> i32 {
        let target_path = PathBuf::from(command.get_program());
        let target_args = command
            .get_args()
            .collect::<Vec<&OsStr>>()
            .iter()
            .map(|x| x.to_str().unwrap().to_string())
            .collect::<Vec<String>>();
        let virtual_desktop = VirtualDesktop::new();
        let process = virtual_desktop.spawn_process(&target_path, &target_args);
        let exit_code = process.wait();
        virtual_desktop.close();
        exit_code
    }
}
