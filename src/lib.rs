//! Custom Allocator based on buddy System
#![deny(missing_docs)]
#![cfg_attr(all(feature = "no-std", not(test)), no_std)]
#![feature(allocator_api)]
#![feature(strict_provenance)]
#![feature(slice_ptr_get)]
#![feature(const_align_offset)]
#![feature(const_mut_refs)]
#![feature(const_convert)] // for tests
#![feature(const_trait_impl)]
#![feature(generic_const_exprs)]
//#![feature(stmt_expr_attributes)]
//#![feature(const_slice_index)]
//#![feature(const_try)]
//#![feature(const_option)]
//#![feature(const_num_from_num)]
//#![feature(const_result_drop)]
//#![feature(const_eval_limit)] // https://github.com/rust-lang/rust/issues/93481
//#![const_eval_limit = "0"]

mod inner_allocator;
mod mutex;
#[cfg(test)]
mod tests;

use core::alloc::{AllocError, Allocator, GlobalAlloc, Layout};
use core::marker::PhantomData;
use core::ops::Deref;
#[cfg(feature = "no-std")]
use core::ptr::null_mut;
use core::ptr::NonNull;
#[cfg(not(feature = "no-std"))]
use std::alloc::handle_alloc_error;

/// These traits are exported to implement with your own Mutex
pub use mutex::RwMutex;

pub use inner_allocator::{BuddyError, InnerAllocator};
pub use inner_allocator::{MAX_SUPPORTED_ALIGN, MIN_BUDDY_NB, MIN_CELL_LEN};

/// Buddy Allocator
#[repr(C, align(16))]
pub struct ThreadSafeAllocator<
    'a,
    T: Deref<Target = ProtectedAllocator<'a, X, M>> + Send + Sync + Clone,
    X: RwMutex<InnerAllocator<'a, M>> + Send + Sync,
    const M: usize,
> {
    protected_allocator: T,
    phantom: PhantomData<&'a X>,
}

impl<'a, T, X, const M: usize> ThreadSafeAllocator<'a, T, X, M>
where
    T: Deref<Target = ProtectedAllocator<'a, X, M>> + Send + Sync + Clone,
    X: RwMutex<InnerAllocator<'a, M>> + Send + Sync,
{
    /// Create a new Buddy Allocator
    pub fn new(protected_allocator: T) -> Self {
        Self {
            protected_allocator,
            phantom: PhantomData,
        }
    }
    /// Allocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, BuddyError> {
        self.protected_allocator.allocate(layout)
    }
    /// Deallocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) -> Result<(), BuddyError> {
        self.protected_allocator.deallocate(ptr, layout)
    }
    /// Attempts to shrink the memory block
    #[inline(always)]
    pub fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, BuddyError> {
        self.protected_allocator.shrink(ptr, old_layout, new_layout)
    }
    /// Attempts to extend the memory block.
    #[inline(always)]
    pub fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
        zeroed: bool,
    ) -> Result<NonNull<[u8]>, BuddyError> {
        self.protected_allocator
            .grow(ptr, old_layout, new_layout, zeroed)
    }
    /// TODO
    #[inline(always)]
    pub fn reserve(&self, index: usize, size: usize) -> Result<(), BuddyError> {
        self.protected_allocator.reserve(index, size)
    }
    /// TODO
    #[inline(always)]
    pub fn unreserve(&self, index: usize) -> Result<(), BuddyError> {
        self.protected_allocator.unreserve(index)
    }
}

/// Clone Boilerplate for ThreadSafeAllocator<'a, T, X, M>... - Cannot Derive Naturaly
impl<'a, T, X, const M: usize> Clone for ThreadSafeAllocator<'a, T, X, M>
where
    T: Deref<Target = ProtectedAllocator<'a, X, M>> + Send + Sync + Clone,
    X: RwMutex<InnerAllocator<'a, M>> + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            protected_allocator: self.protected_allocator.clone(),
            phantom: PhantomData,
        }
    }
}

unsafe impl<'a, T, X, const M: usize> Allocator for ThreadSafeAllocator<'a, T, X, M>
where
    T: Deref<Target = ProtectedAllocator<'a, X, M>> + Send + Sync + Clone,
    X: RwMutex<InnerAllocator<'a, M>> + Send + Sync,
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate(layout).map_err(|e| e.into())
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocate(ptr, layout).unwrap();
    }
    // unsafe fn shrink(
    //     &self,
    //     ptr: NonNull<u8>,
    //     old_layout: Layout,
    //     new_layout: Layout,
    // ) -> Result<NonNull<[u8]>, AllocError> {
    //     self.shrink(ptr, old_layout, new_layout)
    //         .map_err(|e| e.into())
    // }
    // unsafe fn grow(
    //     &self,
    //     ptr: NonNull<u8>,
    //     old_layout: Layout,
    //     new_layout: Layout,
    // ) -> Result<NonNull<[u8]>, AllocError> {
    //     self.grow(ptr, old_layout, new_layout, false)
    //         .map_err(|e| e.into())
    // }
    // unsafe fn grow_zeroed(
    //     &self,
    //     ptr: NonNull<u8>,
    //     old_layout: Layout,
    //     new_layout: Layout,
    // ) -> Result<NonNull<[u8]>, AllocError> {
    //     self.grow(ptr, old_layout, new_layout, true)
    //         .map_err(|e| e.into())
    // }
}

/// Static Buddy Allocator
#[repr(C, align(16))]
pub struct ProtectedAllocator<'a, X, const M: usize>
where
    X: RwMutex<InnerAllocator<'a, M>>,
{
    inner_allocator: X,
    error_hook: Option<fn(BuddyError) -> ()>,
    phantom: PhantomData<&'a X>,
}

impl<'a, X, const M: usize> ProtectedAllocator<'a, X, M>
where
    X: RwMutex<InnerAllocator<'a, M>>,
{
    /// Attach a previously allocated chunk generated by create_static_memory_area()
    pub const fn new(mutex_of_inner_allocator: X, error_hook: Option<fn(BuddyError)>) -> Self {
        Self {
            inner_allocator: mutex_of_inner_allocator,
            error_hook,
            phantom: PhantomData,
        }
    }
    /// Allocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, BuddyError> {
        self.inner_allocator
            .lock_mut(|r| r.alloc(layout).map_err(|e| self.check(e)))
            .unwrap()
    }
    /// dellocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) -> Result<(), BuddyError> {
        self.inner_allocator
            .lock_mut(|r| r.dealloc(ptr, layout).map_err(|e| self.check(e)))
            .unwrap()
    }
    /// Attempts to shrink the memory block
    #[inline(always)]
    pub fn shrink(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
    ) -> Result<NonNull<[u8]>, BuddyError> {
        self.inner_allocator
            .lock_mut(|r| {
                r.shrink(ptr, old_layout, new_layout)
                    .map_err(|e| self.check(e))
            })
            .unwrap()
    }
    /// Attempts to extend the memory block
    #[inline(always)]
    pub fn grow(
        &self,
        ptr: NonNull<u8>,
        old_layout: Layout,
        new_layout: Layout,
        zeroed: bool,
    ) -> Result<NonNull<[u8]>, BuddyError> {
        self.inner_allocator
            .lock_mut(|r| {
                r.grow(ptr, old_layout, new_layout, zeroed)
                    .map_err(|e| self.check(e))
            })
            .unwrap()
    }
    /// TODO
    #[inline(always)]
    pub fn reserve(&self, index: usize, size: usize) -> Result<(), BuddyError> {
        self.inner_allocator
            .lock_mut(|r| r.reserve(index, size).map_err(|e| self.check(e)))
            .unwrap()
    }
    /// TODO
    #[inline(always)]
    pub fn unreserve(&self, index: usize) -> Result<(), BuddyError> {
        self.inner_allocator
            .lock_mut(|r| r.unreserve(index).map_err(|e| self.check(e)))
            .unwrap()
    }
    #[inline(always)]
    fn check(&self, error: BuddyError) -> BuddyError {
        if let Some(error_hook) = self.error_hook {
            error_hook(error);
        }
        error
    }
}

unsafe impl<'a, X, const M: usize> Allocator for ProtectedAllocator<'a, X, M>
where
    X: RwMutex<InnerAllocator<'a, M>>,
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate(layout).map_err(|e| e.into())
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocate(ptr, layout).unwrap();
    }
    // unsafe fn shrink(
    //     &self,
    //     ptr: NonNull<u8>,
    //     old_layout: Layout,
    //     new_layout: Layout,
    // ) -> Result<NonNull<[u8]>, AllocError> {
    //     self.shrink(ptr, old_layout, new_layout)
    //         .map_err(|e| e.into())
    // }
    // unsafe fn grow(
    //     &self,
    //     ptr: NonNull<u8>,
    //     old_layout: Layout,
    //     new_layout: Layout,
    // ) -> Result<NonNull<[u8]>, AllocError> {
    //     self.grow(ptr, old_layout, new_layout, false)
    //         .map_err(|e| e.into())
    // }
    // unsafe fn grow_zeroed(
    //     &self,
    //     ptr: NonNull<u8>,
    //     old_layout: Layout,
    //     new_layout: Layout,
    // ) -> Result<NonNull<[u8]>, AllocError> {
    //     self.grow(ptr, old_layout, new_layout, true)
    //         .map_err(|e| e.into())
    // }
}

unsafe impl<'a, X, const M: usize> GlobalAlloc for ProtectedAllocator<'a, X, M>
where
    X: RwMutex<InnerAllocator<'a, M>>,
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.allocate(layout) {
            Ok(non_null) => non_null.as_mut_ptr(),
            Err(_e) => handle_global_alloc_error(layout),
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocate(NonNull::new(ptr).unwrap(), layout).unwrap();
    }
    // unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    //     let new_layout = Layout::from_size_align(new_size, layout.align());
    //     match new_layout {
    //         Err(_) => handle_global_alloc_error(layout),
    //         Ok(new_layout) => {
    //             let result = if new_layout.size() > layout.size() {
    //                 self.grow(NonNull::new(ptr).unwrap(), layout, new_layout, false)
    //             } else {
    //                 self.shrink(NonNull::new(ptr).unwrap(), layout, new_layout)
    //             };
    //             match result {
    //                 Ok(non_null) => non_null.as_mut_ptr(),
    //                 Err(_e) => handle_global_alloc_error(layout),
    //             }
    //         }
    //     }
    // }
}

fn handle_global_alloc_error(layout: Layout) -> *mut u8 {
    #[cfg(not(feature = "no-std"))]
    handle_alloc_error(layout);
    #[cfg(feature = "no-std")]
    null_mut()
}

#[allow(unused_variables)]
impl From<BuddyError> for AllocError {
    #[inline(always)]
    fn from(error: BuddyError) -> Self {
        AllocError
    }
}

// TODO: Reserve blocks
// TODO: design Realloc & Shrink
// TODO: Draw nodes to explain the Buddy research update tree
// TODO: Create test of allowing more memory space to be addressable
// TODO: Create good documentations

// /// Used only for debug purposes
// #[cfg(not(feature = "no-std"))]
// #[allow(dead_code)]
// fn debug(&self) {
//     self.data
//         .lock_mut(|refer| {
//             for (i, v) in refer.0.iter().enumerate() {
//                 print!("{:02x} ", v);
//                 if i != 0 && (i + 1) % 32 == 0 {
//                     println!();
//                 }
//             }
//             println!();
//         })
//         .unwrap();
// }

// #![cfg_attr(all(feature = "no-std", not(test)), feature(alloc_error_handler))]
// #[cfg(all(feature = "no-std", not(test)))]
// #[alloc_error_handler]
// fn out_of_memory(_: core::alloc::Layout) -> ! {
//      panic!("Sa mere");
// }
// ___ Testing on 64bits system Linux (with address sanitizer) ___
// RUST_BACKTRACE=1 RUSTFLAGS=-Zsanitizer=address cargo test -Zbuild-std --target x86_64-unknown-linux-gnu
// ___ Testing on 32bits system Linux (address sanitizer is unaivalable for this arch) ___
// RUST_BACKTRACE=1 cargo test --target i686-unknown-linux-gnu
