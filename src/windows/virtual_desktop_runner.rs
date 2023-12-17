use std::os::windows::io::AsRawHandle;
use std::path::PathBuf;
use std::{
    mem::{self, MaybeUninit},
    ptr,
};
use uuid::Uuid;
use widestring::U16CString;
use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::{
    CloseHandle, BOOL, HANDLE, NTSTATUS, STATUS_PENDING, WAIT_ABANDONED, WAIT_FAILED,
    WAIT_OBJECT_0, WAIT_TIMEOUT, WIN32_ERROR,
};
use windows::Win32::System::StationsAndDesktops::{
    CloseDesktop, CreateDesktopW, DESKTOP_CONTROL_FLAGS, HDESK,
};
use windows::Win32::System::SystemServices::DESKTOP_CREATEWINDOW;
use windows::Win32::System::Threading::{
    CreateEventW, CreateProcessW, GetExitCodeProcess, SetEvent, TerminateProcess,
    WaitForMultipleObjects, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTF_USESTDHANDLES,
    STARTUPINFOW,
};
use windows::Win32::System::WindowsProgramming::INFINITE;

const WAIT_OBJECT_1: WIN32_ERROR = WIN32_ERROR(WAIT_OBJECT_0.0 + 1);
const STILL_ACTIVE: NTSTATUS = STATUS_PENDING;
const TRUE: BOOL = BOOL(1);
const FALSE: BOOL = BOOL(0);

#[derive(Debug, Clone)]
pub struct VirtualDesktopProcess {
    process_info: PROCESS_INFORMATION,
    cancel_event: HANDLE,
}

impl VirtualDesktopProcess {

    pub fn from_virtual_desktop(
        virtual_desktop: &VirtualDesktop,
        target_path: &PathBuf,
        target_args: &Vec<String>,
    ) -> Self {
        let mut wide_command_line = VirtualDesktopProcess::parse_run_args(target_path, target_args);

        let mut process_info = MaybeUninit::uninit();

        unsafe {
            CreateProcessW(
                PCWSTR::null(),
                PWSTR::from_raw(wide_command_line.as_mut_ptr()),
                None,
                None,
                true,
                CREATE_UNICODE_ENVIRONMENT,
                None,
                None,
                &virtual_desktop.startup_info,
                process_info.as_mut_ptr(),
            )
        }
        .ok()
        .expect("Failed to start process.");

        let process_info = unsafe { process_info.assume_init() };
        unsafe { CloseHandle(process_info.hThread) };
        let cancel_event = unsafe { CreateEventW(None, TRUE, FALSE, PCWSTR::null()) }
            .expect("Failed to create cancel event.");

        ctrlc::set_handler(move || {
            unsafe { SetEvent(cancel_event) }
                .ok()
                .expect("Failed to abort wait for target app exit.")
        })
        .expect("Failed to set Ctrl-C handler.");

        VirtualDesktopProcess {
            process_info,
            cancel_event,
        }
    }

    pub fn parse_run_args(target_path: &PathBuf, target_args: &[String]) -> Vec<u16> {
        let mut wide_command_line = U16CString::from_str(target_path.to_str().unwrap())
            .unwrap()
            .into_vec();
        wide_command_line.push(0);
        for arg in target_args {
            wide_command_line.extend(U16CString::from_str(arg).unwrap().into_vec_with_nul());
        }
        wide_command_line.push(0);
        wide_command_line
    }

    fn fetch_exit_code(&self) -> i32 {
        let mut exit_code = MaybeUninit::uninit();
        unsafe { GetExitCodeProcess(self.process_info.hProcess, exit_code.as_mut_ptr()) }
            .ok()
            .expect("Failed to get target application exit code.");
        let mut exit_code = unsafe { exit_code.assume_init() };
        if exit_code == STILL_ACTIVE.0 as _ {
            unsafe { TerminateProcess(self.process_info.hProcess, 0) }
                .ok()
                .expect("Failed to terminate target application.");
            exit_code = 0;
        }

        exit_code as i32
    }

    pub fn wait(&self) -> i32 {
        let wait_result = unsafe {
            WaitForMultipleObjects(
                &[self.process_info.hProcess, self.cancel_event],
                FALSE,
                INFINITE,
            )
        };
        match wait_result {
            WAIT_OBJECT_0 | WAIT_OBJECT_1 => (),
            WAIT_ABANDONED => unreachable!(),
            WAIT_TIMEOUT => unreachable!(),
            WAIT_FAILED => panic!(
                "Failed to wait for target app exit: {:#?}",
                windows::core::Error::from_win32()
            ),
            _ => unreachable!(),
        }

        self.fetch_exit_code()
    }

    pub fn process_info(&self) -> &PROCESS_INFORMATION {
        &self.process_info
    }

    pub fn cancel_event(&self) -> &HANDLE {
        &self.cancel_event
    }
    
    pub fn kill(&self) {
        unsafe { TerminateProcess(self.process_info.hProcess, 0) }
            .ok()
            .expect("Failed to terminate target application.");
    }

}

#[derive(Clone, Debug, Copy)]
pub struct VirtualDesktop {
    startup_info: STARTUPINFOW,
    virtual_desktop_handle: HDESK,
}

impl VirtualDesktop {
    pub fn new() -> Self {
        let mut virtual_desktop_name =
            U16CString::from_str(format!("virtual-desktop-runner/{}", Uuid::new_v4())).unwrap();

        let virtual_desktop_handle = unsafe {
            CreateDesktopW(
                PCWSTR::from_raw(virtual_desktop_name.as_mut_ptr()),
                None,
                None,
                DESKTOP_CONTROL_FLAGS::default(),
                DESKTOP_CREATEWINDOW.0,
                None,
            )
        }
        .unwrap_or_else(|error| panic!("Failed to create virtual desktop: {:#?}", error));
        let startup_info = STARTUPINFOW {
            cb: mem::size_of::<STARTUPINFOW>() as _,
            lpReserved: PWSTR::null(),
            lpDesktop: PWSTR(virtual_desktop_name.as_mut_ptr()),
            lpTitle: PWSTR::null(),
            dwX: 0,
            dwY: 0,
            dwXSize: 0,
            dwYSize: 0,
            dwXCountChars: 0,
            dwYCountChars: 0,
            dwFillAttribute: 0,
            dwFlags: STARTF_USESTDHANDLES,
            wShowWindow: 0,
            cbReserved2: 0,
            lpReserved2: ptr::null_mut(),
            hStdInput: HANDLE(std::io::stdin().as_raw_handle() as isize),
            hStdOutput: HANDLE(std::io::stdout().as_raw_handle() as isize),
            hStdError: HANDLE(std::io::stderr().as_raw_handle() as isize),
        };

        VirtualDesktop {
            startup_info: startup_info,
            virtual_desktop_handle: virtual_desktop_handle,
        }
    }

    pub fn close(&self) {
        unsafe { CloseDesktop(self.virtual_desktop_handle) }
            .ok()
            .expect("Failed to close virtual desktop.");
    }

    pub fn spawn_process(
        &self,
        target_path: &PathBuf,
        target_args: &Vec<String>,
    ) -> VirtualDesktopProcess {
        VirtualDesktopProcess::from_virtual_desktop(self, target_path, target_args)
    }
}
