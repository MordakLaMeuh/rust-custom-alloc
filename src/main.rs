#![feature(allocator_api)]
#![feature(unchecked_math)]

mod math;

use std::alloc::{handle_alloc_error, AllocError, Allocator, GlobalAlloc, Layout};
use std::ptr::{null, null_mut, NonNull};
use std::sync::Mutex;

/// TODO: Creation must be done when ProtectedAllocator::new is called
const BUDDY: &'static [u8] = &[0_u8; 1024 * 1024];
const MAX_SUPPORTED_ALIGN: usize = 4096;

struct BuddyAllocator {
    allocator: Mutex<ProtectedAllocator>,
}

impl BuddyAllocator {
    pub const fn new(address: *const u8, size: usize) -> Self {
        Self {
            allocator: Mutex::new(ProtectedAllocator::new(address, size)),
        }
    }
}

/// TODO. on final time, this struct must be placed into a choosen memory location
#[repr(C, align(4096))]
struct ProtectedAllocator {
    address: *const u8,
    size: usize,
}

impl ProtectedAllocator {
    const fn new(address: *const u8, size: usize) -> Self {
        Self { address, size }
    }
}

impl ProtectedAllocator {
    fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        // `Layout` contract forbids making a `Layout` with align=0, or align not power of 2.
        // So we can safely use a mask to ensure alignment without worrying about UB.
        let align_mask_to_round_down = !(align - 1);

        if align > MAX_SUPPORTED_ALIGN {
            return null_mut();
        }
        null_mut()
    }
    fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {}
}

unsafe impl Allocator for BuddyAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ptr = self.allocator.lock().unwrap().alloc(layout);
        if ptr.is_null() {
            handle_alloc_error(layout);
        }
        unsafe {
            Ok(NonNull::new_unchecked(std::slice::from_raw_parts_mut(
                ptr,
                layout.size(),
            )))
        }
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.allocator.lock().unwrap().dealloc(ptr.as_ptr(), layout);
    }
}

unsafe impl Sync for BuddyAllocator {}

unsafe impl GlobalAlloc for BuddyAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.allocator.lock().unwrap().alloc(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.allocator.lock().unwrap().dealloc(ptr, layout);
    }
}

// #[global_allocator]
static ALLOCATOR: BuddyAllocator = BuddyAllocator::new(null(), 1024 * 1024 * 32);

fn main() {
    let s = format!("allocating a string!");
    println!("{}", s);

    // let b = Box::new_in(42, &ALLOCATOR);
    // dbg!(b);
}
