//! Custom Allocator based on buddy System
#![deny(missing_docs)]
#![feature(allocator_api)]
#![feature(slice_ptr_get)]
#![feature(const_align_offset)]
#![feature(const_mut_refs)]
#![feature(const_slice_index)]
#![feature(const_option)]
#![feature(const_convert)]
#![feature(const_fmt_arguments_new)]
#![feature(const_trait_impl)]
#![feature(const_num_from_num)]
#![feature(const_result_drop)]
// NOTE: Unwrapping Result<T, E> on const Fn is impossible for the moment
// We use ok() to drop the Result and then just unwrapping the Option<T>
// The associated feature for that is 'const_result_drop'

mod buddy;
mod math;

use buddy::{create_static_chunk, BuddyAllocator, StaticChunk};

use std::alloc::{handle_alloc_error, AllocError, Allocator, GlobalAlloc, Layout};
use std::ptr::NonNull;

// Testing memory
// RUST_BACKTRACE=1 RUSTFLAGS=-Zsanitizer=address cargo run  -Zbuild-std --target x86_64-unknown-linux-gnu
// RUST_BACKTRACE=1 RUSTFLAGS=-Zsanitizer=address cargo test -Zbuild-std --target x86_64-unknown-linux-gnu

unsafe impl<'a> Allocator for BuddyAllocator<'a> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        println!("[Alloc size: {} align: {}]", layout.size(), layout.align());
        dbg!(self.0.lock().unwrap().alloc(layout))
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        println!(
            "[Free size: {} align: {} ptr: {:?}]",
            layout.size(),
            layout.align(),
            ptr
        );
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

const MEMORY_FIELD_SIZE: usize = 1024 * 1024 * 32;

static mut MEMORY_FIELD: StaticChunk<MEMORY_FIELD_SIZE> =
    create_static_chunk::<MEMORY_FIELD_SIZE>();
// #[global_allocator]
static ALLOCATOR: BuddyAllocator =
    BuddyAllocator::attach_static_chunk(unsafe { &mut MEMORY_FIELD });

fn main() {
    println!("struct size: {}", std::mem::size_of::<BuddyAllocator>());
    let s = format!("allocating a string!");
    println!("{}", s);
    println!("ALL - {}", unsafe { MEMORY_FIELD.0[0] });

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
    #[repr(align(4096))]
    struct MemChunk([u8; 2048]);
    let mut chunk = MemChunk([0; 2048]);
    let alloc2 = BuddyAllocator::new(&mut chunk.0);
    dbg!(&alloc2 as *const _);
    let arc = std::sync::Arc::new(alloc2); // Ask to allocate with custom allocator
    dbg!(std::sync::Arc::as_ptr(&arc) as *const _);
    let b = Box::new_in(
        Banane {
            i: 2,
            j: 4,
            k: 8,
            l: 16,
            arr: [42; 8],
        },
        &*arc,
    );
    dbg!(b);

    // fn test<F>(mut c: F)
    // where
    //     F: FnMut()
    // {
    //     c();
    // }
    // let j = 42;
    // let ptr: *mut u8 = std::ptr::null_mut();
    // let c = || {
    // };
    // test(c);
    // dbg!(ptr);

    // block(c);

    // block(|| {
    //     dbg!(j);
    //     dbg!(ptr);

    // });

    // pub fn block<F, T>(f: F) -> ()
    // where
    //     F: FnOnce() -> T,
    //     F: Send + 'static
    // {
    //     f();
    // }
}
