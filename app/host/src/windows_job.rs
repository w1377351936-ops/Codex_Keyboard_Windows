//! A kill-on-close Job Object for the Codex process tree.

#![cfg(windows)]

use std::ffi::c_void;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::process::Child;
use std::ptr;

const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x2000;
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;

#[repr(C)]
#[derive(Default)]
struct BasicLimitInfo {
    per_process_time: i64,
    per_job_time: i64,
    flags: u32,
    minimum_working_set: usize,
    maximum_working_set: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_operations: u64,
    write_operations: u64,
    other_operations: u64,
    read_bytes: u64,
    write_bytes: u64,
    other_bytes: u64,
}

#[repr(C)]
#[derive(Default)]
struct ExtendedLimitInfo {
    basic: BasicLimitInfo,
    io: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
    fn SetInformationJobObject(job: *mut c_void, class: i32, info: *const c_void, length: u32) -> i32;
    fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    fn TerminateJobObject(job: *mut c_void, exit_code: u32) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

pub struct JobObject(*mut c_void);

impl JobObject {
    pub fn attach(child: &Child) -> io::Result<Self> {
        // SAFETY: unnamed job with default security attributes.
        let handle = unsafe { CreateJobObjectW(ptr::null(), ptr::null()) };
        if handle.is_null() {
            return Err(io::Error::last_os_error());
        }
        let job = Self(handle);
        let mut limits = ExtendedLimitInfo::default();
        limits.basic.flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: limits has the layout of JOBOBJECT_EXTENDED_LIMIT_INFORMATION.
        if unsafe {
            SetInformationJobObject(
                job.0,
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                (&limits as *const ExtendedLimitInfo).cast(),
                std::mem::size_of::<ExtendedLimitInfo>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: the child process handle remains valid for this call.
        if unsafe { AssignProcessToJobObject(job.0, child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(job)
    }

    pub fn terminate(&self) {
        // SAFETY: self owns a live Job Object handle; failure is followed by
        // closing the handle, which also terminates assigned processes.
        let _ = unsafe { TerminateJobObject(self.0, 1) };
    }
}

impl Drop for JobObject {
    fn drop(&mut self) {
        // SAFETY: this object owns exactly one handle from CreateJobObjectW.
        let _ = unsafe { CloseHandle(self.0) };
    }
}
