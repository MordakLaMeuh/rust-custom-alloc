//! Custom Allocator based on buddy System
#![deny(missing_docs)]
#![feature(allocator_api)]
#![feature(unchecked_math)]
#![feature(const_align_offset)]
#![feature(const_mut_refs)]
#![feature(slice_ptr_get)]
#![feature(const_slice_index)]
#![feature(const_option)]

mod buddy;
mod math;

use buddy::BuddyAllocator;

use std::alloc::{handle_alloc_error, AllocError, Allocator, GlobalAlloc, Layout};
use std::ptr::NonNull;

// TODO: Creation must be done when ProtectedAllocator::new is called
const MEMORY_FIELD_SIZE: usize = 1024 * 1024 * 32;
#[repr(align(4096))]
struct MemoryField {
    pub array: [u8; MEMORY_FIELD_SIZE],
}
static mut MEMORY_FIELD: MemoryField = MemoryField {
    array: [0; MEMORY_FIELD_SIZE],
};

unsafe impl<'a> Allocator for BuddyAllocator<'a> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0.lock().unwrap().alloc(layout)
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.0.lock().unwrap().dealloc(ptr, layout);
    }
}

unsafe impl<'a> Sync for BuddyAllocator<'a> {}

unsafe impl<'a> GlobalAlloc for BuddyAllocator<'a> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.0.lock().unwrap().alloc(layout) {
            Ok(non_null) => non_null.as_mut_ptr(),
            Err(_) => handle_alloc_error(layout),
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0
            .lock()
            .unwrap()
            .dealloc(NonNull::new(ptr).unwrap(), layout);
    }
}

// #[global_allocator]
static ALLOCATOR: BuddyAllocator = BuddyAllocator::new(unsafe { &mut MEMORY_FIELD.array });

fn main() {
    let s = format!("allocating a string!");
    println!("{}", s);

    #[allow(unused)]
    #[derive(Debug)]
    struct Banane {
        i: u64,
        j: u64,
        k: u64,
        l: u64,
        arr: [u64; 8],
    }
    let b = Box::new_in(
        Banane {
            i: 2,
            j: 4,
            k: 8,
            l: 16,
            arr: [42; 8],
        },
        &ALLOCATOR,
    );
    println!("struct size: {}", std::mem::size_of::<Banane>());
    dbg!(b);
}
