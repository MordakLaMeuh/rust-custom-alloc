//! Custom Allocator based on buddy System
#![deny(missing_docs)]
#![cfg_attr(all(feature = "no-std", not(test)), no_std)]
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
// CFG attributes directly inside function for expressions
#![feature(stmt_expr_attributes)]
// ___ Testing on 64bits system Linux (with address sanitizer) ___
// RUST_BACKTRACE=1 RUSTFLAGS=-Zsanitizer=address cargo test -Zbuild-std --target x86_64-unknown-linux-gnu
// ___ Testing on 32bits system Linux (address sanitizer is unaivalable for this arch) ___
// RUST_BACKTRACE=1 cargo test --target i686-unknown-linux-gnu

mod mutex;
mod protected_allocator;

#[cfg(test)]
mod tests;
// TODO: Draw nodes to explain the Buddy research update tree
// TODO: Select location of buddy Metadata
// TODO: Create test of allowing more memory space to be addressable
// TODO: Reserve blocks
// TODO: Create good documentations
use core::alloc::{AllocError, Allocator, GlobalAlloc, Layout};
use core::marker::PhantomData;
use core::ops::Deref;
#[cfg(feature = "no-std")]
use core::ptr::null_mut;
use core::ptr::NonNull;
#[cfg(not(feature = "no-std"))]
use std::alloc::handle_alloc_error;

/// These traits are exported to implement with your own Mutex
pub use mutex::{GenericMutex, RoMutex, RwMutex};

pub use protected_allocator::ProtectedAllocator;
// #![cfg_attr(all(feature = "no-std", not(test)), feature(alloc_error_handler))]
// #[cfg(all(feature = "no-std", not(test)))]
// #[alloc_error_handler]
// fn out_of_memory(_: core::alloc::Layout) -> ! {
//      panic!("Sa mere");
// }

impl<'a, const M: usize> AddressSpace<'a, M> {
    /// Create a new Address Space
    pub fn new(refer: &'a mut [u8]) -> Self {
        Self(refer)
    }
}

///Wrapper for &mut [u8] witch contains generics declarations
pub struct AddressSpace<'a, const M: usize>(&'a mut [u8]);

/// Buddy Allocator
#[repr(C, align(16))]
pub struct BuddyAllocator<
    'a,
    T: Deref<Target = X> + Send + Sync + Clone,
    X: RwMutex<AddressSpace<'a, M>> + Send + Sync,
    const M: usize,
> {
    data: T,
    phantom: PhantomData<&'a T>,
}

/// Clone Boilerplate for BuddyAllocator<'a, T, X, M>... - Cannot Derive Naturaly
//    = note: the following trait bounds were not satisfied:
//            `Mutex<&mut [u8]>: Clone`
// 113 | #[derive(Clone)]
//     |          ^^^^^ unsatisfied trait bound introduced in this `derive` macro
impl<'a, T, X, const M: usize> Clone for BuddyAllocator<'a, T, X, M>
where
    T: Deref<Target = X> + Send + Sync + Clone,
    X: RwMutex<AddressSpace<'a, M>> + Send + Sync,
{
    fn clone(&self) -> Self {
        Self {
            data: self.data.clone(),
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
        self.allocate(layout)
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocate(ptr, layout);
    }
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
            phantom: PhantomData,
        }
    }
    /// Allocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.data
            .lock_mut(|refer| {
                ProtectedAllocator::<M>(refer.0)
                    .alloc(layout)
                    .map_err(|e| e.into())
            })
            .unwrap()
    }
    /// Deallocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.data
            .lock_mut(|refer| {
                ProtectedAllocator::<M>(refer.0)
                    .dealloc(ptr, layout)
                    .unwrap();
            })
            .unwrap();
    }
    /// Used only for debug purposes
    #[cfg(not(feature = "no-std"))]
    #[allow(dead_code)]
    fn debug(&self) {
        self.data
            .lock_mut(|refer| {
                for (i, v) in refer.0.iter().enumerate() {
                    print!("{:02x} ", v);
                    if i != 0 && (i + 1) % 32 == 0 {
                        println!();
                    }
                }
                println!();
            })
            .unwrap();
    }
}

/// Static Buddy Allocator
#[repr(C, align(16))]
pub struct StaticBuddyAllocator<
    X: GenericMutex<StaticAddressSpace<SIZE, M>>,
    const SIZE: usize,
    const M: usize,
>(X);

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

impl<X: RwMutex<StaticAddressSpace<SIZE, M>>, const SIZE: usize, const M: usize>
    StaticBuddyAllocator<X, SIZE, M>
{
    /// Attach a previously allocated chunk generated by create_static_memory_area()
    pub const fn new(mutex_of_static_address_space: X) -> Self {
        Self(mutex_of_static_address_space)
    }
    /// Allocate memory: should help for a global allocator implementation
    pub fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0
            .lock_mut(|refer| {
                ProtectedAllocator::<M>(&mut refer.0)
                    .alloc(layout)
                    .map_err(|e| e.into())
            })
            .unwrap()
    }
    /// dellocate memory: should help for a global allocator implementation
    pub fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.0
            .lock_mut(|refer| {
                ProtectedAllocator::<M>(&mut refer.0)
                    .dealloc(ptr, layout)
                    .unwrap();
            })
            .unwrap();
    }
}

unsafe impl<X: RwMutex<StaticAddressSpace<SIZE, M>>, const SIZE: usize, const M: usize> Allocator
    for StaticBuddyAllocator<X, SIZE, M>
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate(layout)
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocate(ptr, layout)
    }
}

unsafe impl<X: RwMutex<StaticAddressSpace<SIZE, M>>, const SIZE: usize, const M: usize> GlobalAlloc
    for StaticBuddyAllocator<X, SIZE, M>
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.allocate(layout) {
            Ok(non_null) => non_null.as_mut_ptr(),
            Err(_) => {
                #[cfg(not(feature = "no-std"))]
                handle_alloc_error(layout);
                #[cfg(feature = "no-std")]
                null_mut()
            }
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.deallocate(NonNull::new(ptr).unwrap(), layout);
    }
}

#[allow(unused_variables)]
impl const From<protected_allocator::Error> for AllocError {
    #[inline(always)]
    fn from(error: protected_allocator::Error) -> Self {
        // Dump something ?
        AllocError
    }
}
