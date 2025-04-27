## 简单总结你实现的功能（200字以内，不要贴代码）。

1. 实现 `sys_spawn()` 

内部首先通过 get_app_data_by_name 获得对应 app 的 elf_data, 然后创建对应的 task_control_block, 要注意的是，一开始直接使用 new() 方法，但 new()　只有在创建初始进程的时候才可以，对应的实现从　fork 和 new，exec 方法中可以借鉴，同样要维护对应的父子关系，

2. 实现 `stride`

对应 `TaskControlBlockInner` 添加了一个 priority 和 stride 字段，在每一次 fetch 的时候寻找最小的 stride 并更新 stride

## 问答作业

stride 算法原理非常简单，但是有一个比较大的问题。例如两个 pass = 10 的进程，使用 8bit 无符号整形储存 stride， p1.stride = 255, p2.stride = 250，在 p2 执行一个时间片后，理论上下一次应该 p1 执行。

+ 实际情况是轮到 p1 执行吗？为什么？
+ 实际情况下会错误地轮到 p2 继续执行，因为 stride（比如 u8）时发生了整数溢出

我们之前要求进程优先级 >= 2 其实就是为了解决这个问题。可以证明， 在不考虑溢出的情况下 , 在进程优先级全部 >= 2 的情况下，如果严格按照算法执行，那么 STRIDE_MAX – STRIDE_MIN <= BigStride / 2。

+ 为什么？尝试简单说明（不要求严格证明）。
  所有进程的 pass >= 2，也就是说：stride_increment = BigStride / pass <= BigStride / 2; 
  每次调度一个进程，它的 stride += stride_increment; 
  所有进程轮流调度，每次都是选 stride 最小的那个

```rust
use core::cmp::Ordering;

struct Stride(u64);

impl PartialOrd for Stride {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        const BIG_STRIDE: u64 = u64::MAX; // 假设最大 stride 可为 u64::MAX
        let half = BIG_STRIDE / 2;
        if self.0.wrapping_sub(other.0) < half {
            Some(Ordering::Less)
        } else {
            Some(Ordering::Greater)
        }
    }
}

impl PartialEq for Stride {
    fn eq(&self, _other: &Self) -> bool {
        false // 题目给定假设：永不相等
    }
}
```

## 荣誉准则

在完成本次实验的过程（含此前学习的过程）中，我曾分别与 以下各位 就（与本次实验相关的）以下方面做过交流，还在代码中对应的位置以注释形式记录了具体的交流对象及内容：

ChatGpt

此外，我也参考了 以下资料 ，还在代码中对应的位置以注释形式记录了具体的参考来源及内容：

ChatGpt

1. 我独立完成了本次实验除以上方面之外的所有工作，包括代码与文档。 我清楚地知道，从以上方面获得的信息在一定程度上降低了实验难度，可能会影响起评分。

2. 我从未使用过他人的代码，不管是原封不动地复制，还是经过了某些等价转换。 我未曾也不会向他人（含此后各届同学）复制或公开我的实验代码，我有义务妥善保管好它们。 我提交至本实验的评测系统的代码，均无意于破坏或妨碍任何计算机系统的正常运转。 我清楚地知道，以上情况均为本课程纪律所禁止，若违反，对应的实验成绩将按“-100”分计。