#![cfg_attr(feature = "allocator_trait", feature(allocator_api))]

#[cfg(feature = "allocator_trait")]
mod alloc_api;

use std::alloc::{GlobalAlloc, Layout};
use std::cell::UnsafeCell;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};

pub const ARENA_SIZE: usize = 128 * 1024;
const MAX_SUPPORTED_ALIGN: usize = 4096;

#[repr(C, align(4096))] // 4096 == MAX_SUPPORTED_ALIGN
pub struct SimpleAllocator {
    arena: UnsafeCell<[u8; ARENA_SIZE]>,
    pub remaining: AtomicUsize, // we allocate from the top, counting down
}

impl SimpleAllocator {
    pub const fn init() -> Self {
        Self {
            arena: UnsafeCell::new([0x55; ARENA_SIZE]),
            remaining: AtomicUsize::new(ARENA_SIZE),
        }
    }
}

unsafe impl Sync for SimpleAllocator {}

unsafe impl GlobalAlloc for SimpleAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // `Layout` contract forbids making a `Layout` with align=0, or align not power of 2.
        // So we can safely use a mask to ensure alignment without worrying about UB.
        let align_mask_to_round_down = !(align - 1);

        if align > MAX_SUPPORTED_ALIGN {
            return null_mut();
        }

        let mut allocated = 0;
        if self
            .remaining
            .fetch_update(SeqCst, SeqCst, |mut remaining| {
                if size > remaining {
                    return None;
                }
                remaining -= size;
                remaining &= align_mask_to_round_down;
                allocated = remaining;
                Some(remaining)
            })
            .is_err()
        {
            return null_mut();
        };
        (self.arena.get() as *mut u8).add(allocated)
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(0, 0);
    }
}
