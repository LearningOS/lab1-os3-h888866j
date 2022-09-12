//! Implementation of [`FrameAllocator`] which 
//! controls all the frames in the operating system.

use super::{PhysAddr, PhysPageNum};
use crate::config::MEMORY_END;
use crate::sync::UPSafeCell;
use alloc::vec::Vec;
use core::fmt::{self, Debug, Formatter};
use lazy_static::*;

/// manage a frame which has the same lifecycle as the tracker
pub struct FrameTracker {
    pub ppn: PhysPageNum,
}

impl FrameTracker {
    pub fn new(ppn: PhysPageNum) -> Self {
        // page cleaning
        let bytes_array = ppn.get_bytes_array();
        for i in bytes_array {
            *i = 0;
        }
        Self { ppn }
    }
}

impl Debug for FrameTracker {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_fmt(format_args!("FrameTracker:PPN={:#x}", self.ppn.0))
    }
}

impl Drop for FrameTracker {
    fn drop(&mut self) {
        frame_dealloc(self.ppn);
    }
}

trait FrameAllocator {
    fn new() -> Self;
    fn alloc(&mut self) -> Option<PhysPageNum>;
    fn dealloc(&mut self, ppn: PhysPageNum);
}

/// an implementation for frame allocator
pub struct StackFrameAllocator {
    current: usize,
    end: usize,
    recycled: Vec<usize>,
}

impl StackFrameAllocator {
    /// init 调用的时候 进行初始化，传入物理页号区间
    pub fn init(&mut self, l: PhysPageNum, r: PhysPageNum) {
        self.current = l.0;
        self.end = r.0;
    }
}
impl FrameAllocator for StackFrameAllocator {
    /// new 构建`StackFrameAllocator`实例, new 的时候默认都是 0， 
    /// 后面 init 调用的时候再初始化，传入物理页号区间
    fn new() -> Self {
        Self {
            current: 0,
            // new 的时候默认都是 0， 后面 init 调用的时候再初始化，传入物理页号区间
            end: 0,
            recycled: Vec::new(),
        }
    }
    /// Frame 分配方法，从已回收 Frame 的 vec 中取用一个重新分配出去，
    /// 或者从 current 开始分配一个新的 frame，current+=1
    fn alloc(&mut self) -> Option<PhysPageNum> {
        // 如果已回收待分配使用的 vec 中有，就有限分配 它
        if let Some(ppn) = self.recycled.pop() {
            Some(ppn.into())
        } else if self.current == self.end {
            // 分配完了，先返回 None
            None
        } else {
            // 分配一个 frame，生成一个 Some(PhysPageNum)
            self.current += 1;
            Some((self.current - 1).into())
        }
    }
    /// 回收
    fn dealloc(&mut self, ppn: PhysPageNum) {
        let ppn = ppn.0;
        // validity check
        // ppn 应该小于 current，且 ppn 不在已回收的 vec 里面， 不满足则 panic
        if ppn >= self.current || self.recycled.iter().any(|v| *v == ppn) {
            panic!("Frame ppn={:#x} has not been allocated!", ppn);
        }
        // recycle
        // 回收，留待以后重新分配出去使用
        self.recycled.push(ppn);
    }
}

/// 类型别名 `FrameAllocatorImpl` 就是 `StackFrameAllocator`, 内部有 current，end，recycled 三个字段
/// 
/// current 是未分配的 ppn:usize 起始位置
/// 
/// end 是空间上限 ppn 位置
/// `
/// recycled 是已回收 frame 的 ppn  vector:Vec<usize>
type FrameAllocatorImpl = StackFrameAllocator;

lazy_static! {
    /// frame allocator instance through lazy_static!
    /// 
    /// 还没进行初始化，需要调用 `init` 进行初始化， `init` 又包装在公共函数 `init_frame_allocator` 里
    pub static ref FRAME_ALLOCATOR: UPSafeCell<FrameAllocatorImpl> =
        unsafe { UPSafeCell::new(FrameAllocatorImpl::new()) };
}

/// initiate the frame allocator using `ekernel` as left and `MEMORY_END` as right
pub fn init_frame_allocator() {
    extern "C" {
        fn ekernel();
    }
    FRAME_ALLOCATOR.exclusive_access().init(
        PhysAddr::from(ekernel as usize).ceil(),
        PhysAddr::from(MEMORY_END).floor(),
    );
}

/// allocate a frame
/// 
/// 调用 FRAME_ALLOCATOR 的 `alloc` 方法，
/// 
/// 然后根据其返回值 `Option<PhysPageNum>` 创建一个 `Option<FrameTracker>` 并返回它
/// 
/// `FrameTracker` 创建时包含了对这个 frame 的清空操作
pub fn frame_alloc() -> Option<FrameTracker> {
    FRAME_ALLOCATOR
        .exclusive_access()
        .alloc()
        .map(FrameTracker::new)
}

/// deallocate a frame
/// call StackFrameAllocator's dealloc method to perform this action
fn frame_dealloc(ppn: PhysPageNum) {
    FRAME_ALLOCATOR.exclusive_access().dealloc(ppn);
}

#[allow(unused)]
/// a simple test for frame allocator
pub fn frame_allocator_test() {
    let mut v: Vec<FrameTracker> = Vec::new();
    for i in 0..5 {
        let frame = frame_alloc().unwrap();
        info!("{:?}", frame);
        v.push(frame);
    }
    v.clear();
    for i in 0..5 {
        let frame = frame_alloc().unwrap();
        info!("{:?}", frame);
        v.push(frame);
    }
    drop(v);
    info!("frame_allocator_test passed!");
}
