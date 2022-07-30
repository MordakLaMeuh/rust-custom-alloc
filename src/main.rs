//! Custom Allocator based on buddy System
#![deny(missing_docs)]
// Allow use of custom allocator
#![feature(allocator_api)]
// Get a pointer from the beginning of the slice
#![feature(slice_ptr_get)]
// Const fn align_offset of std::ptr
#![feature(const_align_offset)]
// Use of mutable reference into const fn
#![feature(const_mut_refs)]
// Allow to use Index and IndexMut on slices in const fn
#![feature(const_slice_index)]
// Use of Option<T> in const fn
#![feature(const_option)]
// Used for impl TryFrom boilerplates in const fn
#![feature(const_convert)]
// Allow to writes boilerplates with const before impl keyword
#![feature(const_trait_impl)]
// Allow use of Try operator ? on Result in const fn
#![feature(const_try)]
// allow to use From and Into on Integer an float types in const Fn
#![feature(const_num_from_num)]
// NOTE: Unwrapping Result<T, E> on const Fn is impossible for the moment
// We use ok() to drop the Result and then just unwrapping the Option<T>
// The associated feature for that is 'const_result_drop'
#![feature(const_result_drop)]
// Allow to use const_looping see: https://github.com/rust-lang/rust/issues/93481
#![feature(const_eval_limit)]
#![const_eval_limit = "0"]
// Allow to use addr() fm on std::ptr
#![feature(strict_provenance)]
// Testing memory
// RUST_BACKTRACE=1 RUSTFLAGS=-Zsanitizer=address cargo run  -Zbuild-std --target x86_64-unknown-linux-gnu
// RUST_BACKTRACE=1 RUSTFLAGS=-Zsanitizer=address cargo test -Zbuild-std --target x86_64-unknown-linux-gnu

mod buddy;
mod math;

use buddy::{create_static_chunk, BuddyAllocator, StaticChunk};

use std::alloc::{handle_alloc_error, AllocError, Allocator, GlobalAlloc, Layout};
use std::ptr::NonNull;

unsafe impl<'a> Allocator for BuddyAllocator<'a> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        println!("[Alloc size: {} align: {}]", layout.size(), layout.align());
        self.debug();
        let out = self.0.lock().unwrap().alloc(layout);
        self.debug();
        out
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        println!(
            "[Free size: {} align: {} ptr: {:?}]",
            layout.size(),
            layout.align(),
            ptr
        );
        self.debug();
        self.0.lock().unwrap().dealloc(ptr, layout);
        self.debug();
    }
}

unsafe impl<'a> Sync for BuddyAllocator<'a> {}

unsafe impl<'a> GlobalAlloc for BuddyAllocator<'a> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // println!("[Alloc size: {} align: {}]", layout.size(), layout.align());
        // self.debug();
        let out = match self.0.lock().unwrap().alloc(layout) {
            Ok(non_null) => non_null.as_mut_ptr(),
            Err(_) => handle_alloc_error(layout),
        };
        // self.debug();
        out
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // println!(
        //     "[Free size: {} align: {} ptr: {:?}]",
        //     layout.size(),
        //     layout.align(),
        //     ptr
        // );
        // self.debug();
        self.0
            .lock()
            .unwrap()
            .dealloc(NonNull::new(ptr).unwrap(), layout);
        // self.debug();
    }
}

const MEMORY_FIELD_SIZE: usize = 512 * 1024 * 32;

static mut MEMORY_FIELD: StaticChunk<MEMORY_FIELD_SIZE> =
    create_static_chunk::<MEMORY_FIELD_SIZE>();
#[global_allocator]
static ALLOCATOR: BuddyAllocator =
    BuddyAllocator::attach_static_chunk(unsafe { &mut MEMORY_FIELD });

fn main() {
    println!("struct size: {}", std::mem::size_of::<BuddyAllocator>());
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
    println!("struct size: {}", std::mem::size_of::<Banane>());
    #[repr(align(4096))]
    struct MemChunk([u8; 256]);
    let mut chunk = MemChunk([0; 256]);
    let alloc2 = BuddyAllocator::new(&mut chunk.0);
    dbg!(&alloc2 as *const _);
    let arc = std::sync::Arc::new(alloc2); // Ask to allocate with custom allocator
    dbg!(std::sync::Arc::as_ptr(&arc) as *const _);
    let b = Box::new_in(
        Banane {
            i: 0xAAAAAAAAAAAAAAAA,
            j: 0xBBBBBBBBBBBBBBBB,
            k: 0xCCCCCCCCCCCCCCCC,
            l: 0xDDDDDDDDDDDDDDDD,
            arr: [0xFFFFFFFFFFFFFFFF; 8],
        },
        &*arc,
    );
    dbg!(&b);
    let c = Box::new_in(0xAABBCCDD_u32, &*arc);
    dbg!(&c);
    let d = Box::try_new_in(0xAABBCCDD_u32, &*arc);
    dbg!(&d);
    drop(b);
    drop(c);

    #[repr(align(4096))]
    struct MemChunk2([u8; 256]);
    let mut chunk = MemChunk2([0; 256]);
    let alloc = BuddyAllocator::new(&mut chunk.0);

    let mut v = Vec::new();
    for _i in 0..3 {
        let b = Box::try_new_in([0xaa_u8; 64], &alloc);
        if let Err(_e) = &b {
            panic!("Allocation error");
        }
        v.push(b);
    }
    let b = Box::try_new_in([0xaa_u8; 64], &alloc);
    if let Ok(_) = b {
        panic!("Should not allocate again");
    }
    drop(v);
    let b = Box::try_new_in([0xaa_u8; 128], &alloc);
    if let Err(_e) = &b {
        panic!("Allocation error");
    }
}
