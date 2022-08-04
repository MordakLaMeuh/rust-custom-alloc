//! Custom Allocator based on buddy System
#![deny(missing_docs)]
#![cfg_attr(all(feature = "no-std", not(test)), no_std)]
#![feature(allocator_api)]
#![feature(strict_provenance)]
#![feature(stmt_expr_attributes)]
#![feature(slice_ptr_get)]
#![feature(const_align_offset)]
#![feature(const_mut_refs)]
#![feature(const_slice_index)]
#![feature(const_option)]
#![feature(const_convert)]
#![feature(const_trait_impl)]
#![feature(const_try)]
#![feature(const_num_from_num)]
#![feature(const_result_drop)]
#![feature(const_eval_limit)] // https://github.com/rust-lang/rust/issues/93481
#![const_eval_limit = "0"]

mod mutex;
mod protected_allocator;
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

pub use protected_allocator::BuddyError;
pub use protected_allocator::ProtectedAllocator;
pub use protected_allocator::{MAX_SUPPORTED_ALIGN, MIN_BUDDY_NB, MIN_CELL_LEN};

impl<'a, const M: usize> AddressSpace<'a, M> {
    /// Create a new Address Space
    pub fn new(refer: &'a mut [u8]) -> Self {
        Self(refer)
    }
}

///Wrapper for &mut \[[u8]\] witch contains generics declarations
pub struct AddressSpace<'a, const M: usize>(pub &'a mut [u8]);

/// Buddy Allocator
#[repr(C, align(16))]
pub struct BuddyAllocator<
    'a,
    T: Deref<Target = X> + Send + Sync + Clone,
    X: RwMutex<AddressSpace<'a, M>> + Send + Sync,
    const M: usize,
> {
    data: T,
    error_hook: Option<fn(BuddyError) -> ()>,
    phantom: PhantomData<&'a T>,
}

impl<'a, T, X, const M: usize> BuddyAllocator<'a, T, X, M>
where
    T: Deref<Target = X> + Send + Sync + Clone,
    X: RwMutex<AddressSpace<'a, M>> + Send + Sync,
{
    /// Create a new Buddy Allocator
    pub fn new(content: T) -> Self {
        content
            .lock_mut(|refer| {
                ProtectedAllocator::<M>(refer.0).init();
            })
            .unwrap();
        Self {
            data: content,
            error_hook: None,
            phantom: PhantomData,
        }
    }
    /// Allocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, BuddyError> {
        self.data
            .lock_mut(|refer| ProtectedAllocator::<M>(refer.0).alloc(layout))
            .unwrap()
    }
    /// Deallocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) -> Result<(), BuddyError> {
        self.data
            .lock_mut(|refer| ProtectedAllocator::<M>(refer.0).dealloc(ptr, layout))
            .unwrap()
    }
    // fn reserve(&self, index: usize, size: usize) -> Result<(), BuddyError> {
    //     unimplemented!();
    // }
    // fn unreserve(&self, index: usize) -> Result<(), BuddyError> {
    //     unimplemented!();
    // }
    /// Set the error hook
    pub fn set_error_hook(&mut self, c: fn(BuddyError)) {
        self.error_hook = Some(c);
    }
}

/// Clone Boilerplate for BuddyAllocator<'a, T, X, M>... - Cannot Derive Naturaly
impl<'a, T, X, const M: usize> Clone for BuddyAllocator<'a, T, X, M>
where
    T: Deref<Target = X> + Send + Sync + Clone,
    X: RwMutex<AddressSpace<'a, M>> + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
            error_hook: self.error_hook.clone(),
            phantom: PhantomData,
        }
    }
}

unsafe impl<'a, T, X, const M: usize> Allocator for BuddyAllocator<'a, T, X, M>
where
    T: Deref<Target = X> + Send + Sync + Clone,
    X: RwMutex<AddressSpace<'a, M>> + Send + Sync,
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate(layout).map_err(|e| {
            if let Some(error_hook) = self.error_hook {
                error_hook(e);
            }
            e.into()
        })
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocate(ptr, layout)
            .map_err(|e| {
                if let Some(error_hook) = self.error_hook {
                    error_hook(e);
                }
                e
            })
            .unwrap();
    }
}

/// Static Buddy Allocator
#[repr(C, align(16))]
pub struct StaticBuddyAllocator<
    X: RwMutex<StaticAddressSpace<SIZE, M>>,
    const SIZE: usize,
    const M: usize,
> {
    data: X,
    error_hook: Option<fn(BuddyError) -> ()>,
}

/// Use only for static allocation
#[repr(align(4096))]
pub struct StaticAddressSpace<const SIZE: usize, const M: usize>(pub [u8; SIZE]);

impl<const SIZE: usize, const M: usize> StaticAddressSpace<SIZE, M> {
    /// Helper to create static const address space for allocations
    /// Be carefull, static chunks affect hugely the executable's size
    pub const fn new() -> Self {
        let mut area: [u8; SIZE] = [0; SIZE];
        ProtectedAllocator::<M>(&mut area).init();
        StaticAddressSpace(area)
    }
}

impl<X, const SIZE: usize, const M: usize> StaticBuddyAllocator<X, SIZE, M>
where
    X: RwMutex<StaticAddressSpace<SIZE, M>>,
{
    /// Attach a previously allocated chunk generated by create_static_memory_area()
    pub const fn new(mutex_of_static_address_space: X) -> Self {
        Self {
            data: mutex_of_static_address_space,
            error_hook: None,
        }
    }
    /// Allocate memory: should help for a global allocator implementation
    pub fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, BuddyError> {
        self.data
            .lock_mut(|refer| ProtectedAllocator::<M>(&mut refer.0).alloc(layout))
            .unwrap()
    }
    /// dellocate memory: should help for a global allocator implementation
    pub fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) -> Result<(), BuddyError> {
        self.data
            .lock_mut(|refer| ProtectedAllocator::<M>(&mut refer.0).dealloc(ptr, layout))
            .unwrap()
    }
    // fn reserve(&self, index: usize, size: usize) -> Result<(), BuddyError> {
    //     unimplemented!();
    // }
    // fn unreserve(&self, index: usize) -> Result<(), BuddyError> {
    //     unimplemented!();
    // }
    /// Set the error hook
    pub fn set_error_hook(&mut self, c: fn(BuddyError)) {
        self.error_hook = Some(c);
    }
}

unsafe impl<X, const SIZE: usize, const M: usize> Allocator for StaticBuddyAllocator<X, SIZE, M>
where
    X: RwMutex<StaticAddressSpace<SIZE, M>>,
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate(layout).map_err(|e| {
            if let Some(error_hook) = self.error_hook {
                error_hook(e);
            }
            e.into()
        })
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocate(ptr, layout)
            .map_err(|e| {
                if let Some(error_hook) = self.error_hook {
                    error_hook(e);
                }
                e
            })
            .unwrap()
    }
}

unsafe impl<X, const SIZE: usize, const M: usize> GlobalAlloc for StaticBuddyAllocator<X, SIZE, M>
where
    X: RwMutex<StaticAddressSpace<SIZE, M>>,
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.allocate(layout) {
            Ok(non_null) => non_null.as_mut_ptr(),
            Err(e) => {
                if let Some(error_hook) = self.error_hook {
                    error_hook(e);
                }
                #[cfg(not(feature = "no-std"))]
                handle_alloc_error(layout);
                #[cfg(feature = "no-std")]
                null_mut()
            }
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocate(NonNull::new(ptr).unwrap(), layout)
            .map_err(|e| {
                if let Some(error_hook) = self.error_hook {
                    error_hook(e);
                }
                e
            })
            .unwrap();
    }
}

#[allow(unused_variables)]
impl const From<BuddyError> for AllocError {
    #[inline(always)]
    fn from(error: BuddyError) -> Self {
        AllocError
    }
}

// TODO: design Realloc & Shrink
// TODO: Draw nodes to explain the Buddy research update tree
// TODO: Select location of buddy Metadata
// TODO: Create test of allowing more memory space to be addressable
// TODO: Reserve blocks
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
