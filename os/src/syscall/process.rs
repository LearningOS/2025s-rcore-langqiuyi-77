//! Process management syscalls
//!
use alloc::sync::Arc;

use crate::{
    fs::{open_file, OpenFlags},
    mm::{translated_refmut, translated_str},
    task::{
        add_task, current_task, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next,
    },
    timer::get_time_us
};
// use riscv::register::fcsr::Flags;

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

pub fn sys_exit(exit_code: i32) -> ! {
    trace!("kernel:pid[{}] sys_exit", current_task().unwrap().pid.0);
    exit_current_and_run_next(exit_code);
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel:pid[{}] sys_yield", current_task().unwrap().pid.0);
    suspend_current_and_run_next();
    0
}

pub fn sys_getpid() -> isize {
    trace!("kernel: sys_getpid pid:{}", current_task().unwrap().pid.0);
    current_task().unwrap().pid.0 as isize
}

pub fn sys_fork() -> isize {
    trace!("kernel:pid[{}] sys_fork", current_task().unwrap().pid.0);
    let current_task = current_task().unwrap();
    let new_task = current_task.fork();
    let new_pid = new_task.pid.0;
    // modify trap context of new_task, because it returns immediately after switching
    let trap_cx = new_task.inner_exclusive_access().get_trap_cx();
    // we do not have to move to next instruction since we have done it before
    // for child process, fork returns 0
    trap_cx.x[10] = 0;
    // add new task to scheduler
    add_task(new_task);
    new_pid as isize
}

pub fn sys_exec(path: *const u8) -> isize {
    trace!("kernel:pid[{}] sys_exec", current_task().unwrap().pid.0);
    let token = current_user_token();
    let path = translated_str(token, path);
    if let Some(app_inode) = open_file(path.as_str(), OpenFlags::RDONLY) {
        let all_data = app_inode.read_all();
        let task = current_task().unwrap();
        task.exec(all_data.as_slice());
        0
    } else {
        -1
    }
}

/// If there is not a child process whose pid is same as given, return -1.
/// Else if there is a child process but it is still running, return -2.
pub fn sys_waitpid(pid: isize, exit_code_ptr: *mut i32) -> isize {
    trace!("kernel::pid[{}] sys_waitpid [{}]", current_task().unwrap().pid.0, pid);
    let task = current_task().unwrap();
    // find a child process

    // ---- access current PCB exclusively
    let mut inner = task.inner_exclusive_access();
    if !inner
        .children
        .iter()
        .any(|p| pid == -1 || pid as usize == p.getpid())
    {
        return -1;
        // ---- release current PCB
    }
    let pair = inner.children.iter().enumerate().find(|(_, p)| {
        // ++++ temporarily access child PCB exclusively
        p.inner_exclusive_access().is_zombie() && (pid == -1 || pid as usize == p.getpid())
        // ++++ release child PCB
    });
    if let Some((idx, _)) = pair {
        let child = inner.children.remove(idx);
        // confirm that child will be deallocated after being removed from children list
        assert_eq!(Arc::strong_count(&child), 1);
        let found_pid = child.getpid();
        // ++++ temporarily access child PCB exclusively
        let exit_code = child.inner_exclusive_access().exit_code;
        // ++++ release child PCB
        *translated_refmut(inner.memory_set.token(), exit_code_ptr) = exit_code;
        found_pid as isize
    } else {
        -2
    }
    // ---- release current PCB automatically
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    // trace!("kernel: sys_get_time");
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

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    // start 是用户传进来的虚拟地址，而你的 mmap 实现要做的，是给用户分配一段映射好的虚拟内存区域。这不是内核用的地址空间，而是 用户进程地址空间中的一部分！
    trace!("kernel: sys_mmap");


    // 错误 1: start 没有页对齐
    if start % PAGE_SIZE != 0 {
        trace!("sys_mmap error: page not align up");
        return -1;
    }

    // 错误 2 + 3: 权限不合法
    let permission = match parse_prot(prot) {
        Some(p) => p,
        None => {
            trace!("sys_mmap error: permission illegal");
            return -1;
        }
    };

    // 错误 4: len 为 0
    if len == 0 {
        trace!("sys_mmap error: len = 0");
        return -1;
    }

    // let len_aligned = page_align_up(len);
    let start_vpn = VirtAddr::from(start).floor();
    let end_vpn = VirtAddr::from(start + len).ceil();

    if let Some(task) = current_task() {
        let tcb = task;
        let mut inner = tcb.inner_exclusive_access();
        let memory_set = &mut inner.memory_set;

        // 错误 5: 重复映射
        if memory_set.overlap_with(start_vpn, end_vpn) {
            trace!("sys_mmap error: overlapped");
            return -1;
        }

        // 加入当前内存映射
        memory_set.insert_framed_area(VirtAddr::from(start), VirtAddr::from(start + len), permission);

        0 // 成功
    } else {
        -1
    }
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(start: usize, len: usize) -> isize {
    trace!("kernel: sys_munmap");
    if start % PAGE_SIZE != 0 || len == 0 {
        return -1;
    }

    // let len_aligned = page_align_up(len);
    let start_vpn = VirtAddr::from(start).floor();
    let end_vpn = VirtAddr::from(start + len).ceil();

    if let Some(task) = current_task() {
        let tcb = task;
        let mut inner = tcb.inner_exclusive_access();
        let memory_set = &mut inner.memory_set;

        // 确保整个区间是一个完整的 MapArea
        if let Some(_area) = memory_set.find_map_area_containing(start_vpn, end_vpn) {
            // 取消映射 + 移除区域描述（MapArea）
            memory_set.unmap_area(start_vpn, end_vpn);
            0
        } else {
            trace!("[munmap] no map area contains the start!");
            -1
        }
    } else {
        -1
    }
}

/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel:pid[{}] sys_sbrk", current_task().unwrap().pid.0);
    if let Some(old_brk) = current_task().unwrap().change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}

/// YOUR JOB: Implement spawn.
/// HINT: fork + exec =/= spawn
pub fn sys_spawn(path: *const u8) -> isize {
    trace!(
        "kernel:pid[{}] sys_spawn",
        current_task().unwrap().pid.0
    );
    // &str 是 (*const u8, usize)
    let token = current_user_token();
    let path = &translated_str(token, path);
    let current_task = current_task().unwrap();
    // 通过 path 获得应用 elf 文件
    // TaskControlBlock.new(elf_data: &[u8]) Create a new process 
    // 内部有初始话该进程地址空间中的 Trap 上下文，使得进入用户态时，可以正确跳转到应用入口点
    // add_task(new_task) 添加到调度器
    if let Some(elf_data) = get_app_data_by_name(path.as_str()) {
        let new_task = current_task.spawn(elf_data);
        let new_pid = new_task.pid.0;
        add_task(new_task);   
        // 0  spawn 不是返回 0 而是要返回对应的 PID
        new_pid as isize
    } else {
        panic!("There is no path faile for spawn to run");
    }
}

// YOUR JOB: Set task priority.
pub fn sys_set_priority(prio: isize) -> isize {
    trace!(
        "kernel:pid[{}] sys_set_priority",
        current_task().unwrap().pid.0
    );
    
    if prio <= 1 {
        // panic!("prio get {} but it should be >= 2", prio);   参数错误，不用 panic，返回 -1 表示错误
        -1  
    } else {
        if let Some(tcb) = current_task() {
            let mut inner = tcb.inner_exclusive_access();
            inner.priority = prio as usize;
            // 0    设置成功返回 prio 而不是 0，不是返回 0 就是成功，要看方法定义
            prio    
        } else {
            panic!("current_task() get None");
        }
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

// fn page_align_up(len: usize) -> usize {
//     (len + PAGE_SIZE - 1) / PAGE_SIZE * PAGE_SIZE
// }



