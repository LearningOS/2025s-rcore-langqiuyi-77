//! Process management syscalls
// use core::result;

use crate::{
    task::{exit_current_and_run_next, suspend_current_and_run_next, TASK_MANAGER},
    timer::get_time_us,
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(exit_code: i32) -> ! {
    trace!("[kernel] Application exited with code {}", exit_code);
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// get time with second and microsecond
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us();
    unsafe {
        *ts = TimeVal {
            sec: us / 1_000_000,
            usec: us % 1_000_000,
        };
    }
    0
}

// TODO: implement the syscall
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    if trace_request == 0 {
        let ptr = id as *const u8;       // 读取数据只需要 const
        let byte: u8 = unsafe { *ptr };             // 裸指针是不安全的，必须使用 unsafe 解引用
        byte as isize
    } else if trace_request == 1 {
        let ptr: *mut u8 = id as *mut u8;         // 要写入数据需要 mut
        let byte: u8 = data as u8;
        unsafe {
            *ptr = byte;
        }
        0
    } else if trace_request == 2 {
        TASK_MANAGER.get_system_call(id)
    } else {
        -1
    }
}
