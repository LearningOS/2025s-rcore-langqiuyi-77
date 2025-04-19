//! Process management syscalls
// use riscv::register::fcsr::Flags;

use crate::{config::PAGE_SIZE, mm::{translated_byte_buffer, MapPermission, PTEFlags, PageTable, VirtAddr}, task::{change_program_brk, current_user_token, exit_current_and_run_next, suspend_current_and_run_next, TASK_MANAGER}, timer::get_time_us};

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

fn parse_prot(prot: usize) -> Option<MapPermission> {
     // 检查是否包含无效位（高于第 2 位）
    if prot & !0x7 != 0 || prot & 0x7 == 0 {
        return None;
    }

    let mut perm = MapPermission::empty();

    if prot & 0x1 != 0 {
        perm |= MapPermission::R;
    }
    if prot & 0x2 != 0 {
        perm |= MapPermission::W;
    }
    if prot & 0x4 != 0 {
        perm |= MapPermission::X;
    }

    perm |= MapPermission::U; // mmap 默认都是用户空间映射

    Some(perm)
}

fn page_align_up(len: usize) -> usize {
    (len + PAGE_SIZE - 1) / PAGE_SIZE * PAGE_SIZE
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    trace!("kernel: sys_mmap NOT IMPLEMENTED YET!");
    // 错误 1: start 没有页对齐
    if start % PAGE_SIZE != 0 {
        return -1;
    }

    // 错误 2 + 3: 权限不合法
    let permission = match parse_prot(prot) {
        Some(p) => p,
        None => return -1,
    };

    // 错误 4: len 为 0
    if len == 0 {
        return -1;
    }

    let len_aligned = page_align_up(len);
    let start_vpn = VirtAddr::from(start).floor();
    let end_vpn = VirtAddr::from(start + len_aligned).ceil();

    let result = TASK_MANAGER.with_current_task_mut(|task_inner| {
        let memory_set = &mut task_inner.memory_set;

        // 错误 5: 重复映射
        if memory_set.overlap_with(start_vpn, end_vpn) {
            return -1;
        }

        // 加入当前内存映射
        memory_set.insert_framed_area(VirtAddr::from(start), VirtAddr::from(start + len_aligned), permission);

        0 // 成功
    });

    result
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    if start % PAGE_SIZE != 0 || len == 0 {
        return -1;
    }

    let len_aligned = page_align_up(len);
    let start_vpn = VirtAddr::from(start).floor();
    let end_vpn = VirtAddr::from(start + len_aligned).ceil();

    let retult = TASK_MANAGER.with_current_task_mut(|task| {
        let memory_set = &mut task.memory_set;
        // 检查所有页都已被映射
        let mut addr = start;
        while addr < start + len {
            let vpn = VirtAddr::from(addr).floor();
            if PageTable::from_token(current_user_token()).translate(vpn).is_none() {
                return -1; // 存在未映射的页
            }
            addr += PAGE_SIZE;
        }

        // 取消映射 + 移除区域描述（MapArea）
        memory_set.unmap_area(start_vpn, end_vpn);

        0
    });
    
    retult
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
