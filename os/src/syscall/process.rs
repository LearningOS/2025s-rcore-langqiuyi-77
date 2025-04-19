//! Process management syscalls
use riscv::register::fcsr::Flags;

use crate::{mm::{translated_byte_buffer, PTEFlags, PageTable, VirtAddr}, task::{change_program_brk, current_user_token, exit_current_and_run_next, suspend_current_and_run_next, TASK_MANAGER}, timer::get_time_us};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us: usize = get_time_us();
    let ptr = ts as *const u8;                   // Rust 的指针之间的类型转换是合法的，只要你不解引用它就没事；
    let len = core::mem::size_of::<TimeVal>();

    let buffers = translated_byte_buffer(current_user_token(), ptr, len);

    let time_val = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };

    // 转成字节数组
    let time_val_bytes = unsafe {
        core::slice::from_raw_parts(
            &time_val as *const _ as *const u8,
            core::mem::size_of::<TimeVal>(),
        )
    };

    // 写进 buffers
    let mut offset = 0;
    for buf in buffers {
        let len = buf.len().min(time_val_bytes.len() - offset);
        buf[..len].copy_from_slice(&time_val_bytes[offset..offset + len]);
        offset += len;
    }

    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(_trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    if _trace_request == 0 {
        // 首先获得对应 id 的 vpn 然后检查对应 pte 来得到权限
        let va = VirtAddr::from(id);
        let vpn = va.floor();
        let page_table = PageTable::from_token(current_user_token());
        let flags = page_table.translate(vpn).unwrap().flags();

        if !flags.contains(PTEFlags::U) {
            return -1; // 不是用户态页，用户不能访问
        }

        if !flags.contains(PTEFlags::R) {
            return -1;
        }
        
        let ptr = id as *const u8;
        let byte = unsafe {
            *ptr
        };
        byte as isize
    } else if _trace_request == 1 {
        let va = VirtAddr::from(id);
        let vpn = va.floor();
        let page_table = PageTable::from_token(current_user_token());
        let flags = page_table.translate(vpn).unwrap().flags();

        if !flags.contains(PTEFlags::U) {
            return -1; 
        }

        if !flags.contains(PTEFlags::W) {
            return -1;
        }

        let ptr: *mut u8 = id as *mut u8;
        let byte: u8 = data as u8;
        unsafe {
            *ptr = byte;
        }
        0
    } else if _trace_request == 2 {
        TASK_MANAGER.get_system_call(id)
    } else {
        -1
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(_start: usize, _len: usize, _port: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    -1
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    -1
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
