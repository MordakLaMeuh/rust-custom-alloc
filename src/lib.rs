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

// ___ Testing on 64bits system Linux (with address sanitizer) ___
// RUST_BACKTRACE=1 RUSTFLAGS=-Zsanitizer=address cargo test -Zbuild-std --target x86_64-unknown-linux-gnu
// ___ Testing on 32bits system Linux (address sanitizer is unaivalable for this arch) ___
// RUST_BACKTRACE=1 cargo test --target i686-unknown-linux-gnu

mod math;
use math::{round_up_2, trailing_zero_right};
#[macro_use]
mod macros;
mod mutex;

#[cfg(test)]
mod tests;
// TODO: Find a solution with no_std
// TODO: Draw nodes to explain the Buddy research update tree
// TODO: Select location of buddy Metadata
// TODO: Allow more memory space to be addressable
// TODO: Reserve blocks
// TODO: Create good documentations
use core::alloc::{AllocError, Allocator, GlobalAlloc, Layout};
use core::marker::PhantomData;
use core::ops::Deref;
#[cfg(all(feature = "no-std", not(test)))]
use core::ptr::null_mut;
use core::ptr::NonNull;
#[cfg(not(feature = "no-std"))]
use std::alloc::handle_alloc_error;

/// These traits are exported to implement with your own Mutex
pub use mutex::{GenericMutex, RoMutex, RwMutex};

// #![cfg_attr(all(feature = "no-std", not(test)), feature(alloc_error_handler))]
// #[cfg(all(feature = "no-std", not(test)))]
// #[alloc_error_handler]
// fn out_of_memory(_: core::alloc::Layout) -> ! {
//      panic!("Sa mere");
// }

const MIN_BUDDY_SIZE: usize = 64;
const MAX_BUDDY_SIZE: usize = 0x2000_0000;
const MAX_SUPPORTED_ALIGN: usize = 4096;

const FIRST_INDEX: usize = 1;

/// Buddy Allocator
#[repr(C, align(16))]
pub struct BuddyAllocator<
    'a,
    T: Deref<Target = X> + Send + Sync + Clone,
    X: RwMutex<&'a mut [u8]> + Send + Sync,
    const M: usize = MIN_BUDDY_SIZE,
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
    X: RwMutex<&'a mut [u8]> + Send + Sync,
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
    X: RwMutex<&'a mut [u8]> + Send + Sync,
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
    X: RwMutex<&'a mut [u8]> + Send + Sync,
{
    /// Create a new Buddy Allocator
    pub fn new(content: T) -> Self {
        content
            .lock_mut(|refer| {
                ProtectedAllocator::<M>(refer).init();
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
            .lock_mut(|refer| ProtectedAllocator::<M>(refer).alloc(layout))
            .unwrap()
    }
    /// Deallocate memory: should help for a global allocator implementation
    #[inline(always)]
    pub fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.data
            .lock_mut(|refer| {
                ProtectedAllocator::<M>(refer).dealloc(ptr, layout);
            })
            .unwrap();
    }
    /// Used only for debug purposes
    #[cfg(not(feature = "no-std"))]
    #[allow(dead_code)]
    fn debug(&self) {
        self.data
            .lock_mut(|refer| {
                for (i, v) in refer.iter().enumerate() {
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
    X: GenericMutex<&'static mut StaticChunk<SIZE, M>>,
    const SIZE: usize,
    const M: usize = MIN_BUDDY_SIZE,
>(X);

/// Use only for static allocation
#[repr(align(4096))]
pub struct StaticChunk<const SIZE: usize, const M: usize>(pub [u8; SIZE]);

/// Helper to create static const chunks for allocations
/// Be carefull, static chunks affect hugely the executable's size
pub const fn create_static_chunk<const SIZE: usize, const M: usize>() -> StaticChunk<SIZE, M> {
    let mut area: [u8; SIZE] = [0; SIZE];
    ProtectedAllocator::<M>(&mut area).init();
    StaticChunk(area)
}

impl<X: RwMutex<&'static mut StaticChunk<SIZE, M>>, const SIZE: usize, const M: usize>
    StaticBuddyAllocator<X, SIZE, M>
{
    /// Attach a previously allocated chunk generated by create_static_memory_area()
    pub const fn attach_static_chunk(mutex: X) -> Self {
        Self(mutex)
    }
    /// Allocate memory: should help for a global allocator implementation
    pub fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.0
            .lock_mut(|refer| ProtectedAllocator::<M>(&mut refer.0).alloc(layout))
            .unwrap()
    }
    /// dellocate memory: should help for a global allocator implementation
    pub fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.0
            .lock_mut(|refer| {
                ProtectedAllocator::<M>(&mut refer.0).dealloc(ptr, layout);
            })
            .unwrap();
    }
}

unsafe impl<X: RwMutex<&'static mut StaticChunk<SIZE, M>>, const SIZE: usize, const M: usize>
    Allocator for StaticBuddyAllocator<X, SIZE, M>
{
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        self.allocate(layout)
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.deallocate(ptr, layout)
    }
}

unsafe impl<X: RwMutex<&'static mut StaticChunk<SIZE, M>>, const SIZE: usize, const M: usize>
    GlobalAlloc for StaticBuddyAllocator<X, SIZE, M>
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

/// Inner part of BuddyAllocator and StaticBuddyAllocator
struct ProtectedAllocator<'a, const M: usize>(pub &'a mut [u8]);

// ___ Requested Buddy Size and Order with their TryFrom<_> boilerplates ___
#[derive(Debug, Copy, Clone)]
struct BuddySize<const M: usize>(usize);
#[derive(Debug, Copy, Clone)]
struct Order(u8);

enum Op {
    Allocate,
    Deallocate,
}

impl<'a, const M: usize> ProtectedAllocator<'a, M> {
    const fn init(&'a mut self) {
        assert!(M >= MIN_BUDDY_SIZE);
        assert!(M <= MAX_BUDDY_SIZE);
        assert!(round_up_2(M as u32) as usize == M);
        assert!(self.0.len() <= MAX_BUDDY_SIZE);
        assert!(self.0.len() >= M * 2);
        let space_rounded_up_2 = round_up_2(self.0.len() as u32) as usize;
        let space_order0_buddy = if space_rounded_up_2 == self.0.len() {
            self.0.len()
        } else {
            space_rounded_up_2 >> 1
        };
        let current_align = if self.0.len() > MAX_SUPPORTED_ALIGN {
            MAX_SUPPORTED_ALIGN
        } else {
            self.0.len()
        };
        let ptr_offset = self.0.as_mut_ptr().align_offset(current_align);
        // IMPORTANT: On compile time with const fn feature, align_offset() doesn't works
        // and returns USIZE::MAX. Trust on you. Can't be sure...
        assert!(ptr_offset == 0 || ptr_offset == usize::MAX); // Check pointer alignement
        let max_order = Order::try_from((BuddySize::<M>(M), BuddySize(space_order0_buddy)))
            .ok()
            .expect("Woot ? Should be already checked !");
        // Bytes needed:       2^(order) * 2
        // order 0.  2o        o X
        // order 1.  4o        o X + X X
        // order 2.  8o        o X + X X + X X X X
        // order 3. 16o        o X + X X + X X X X + X X X X X X X X
        // [..]
        let bytes_needed = (1 << max_order.0) * 2;
        // Cannot use Iterator or IntoIterator in const fn, so we use the C style loop
        // IMPORTANT: A huge problem is that 'bytes_needed' depends of inputs params on const fn
        // it derives from <const SIZE: usize> so address.len(). So We have to hack the compiler to
        // allow 'infinite' eval limit. #![feature(const_eval_limit)] && #![const_eval_limit = "0"]
        // ___ Write original metadatas ___
        let (mut current_order, mut members, mut index) = (0, 2, 0);
        while index < bytes_needed {
            members -= 1;
            self.0[index] = current_order;
            if members == 0 {
                current_order += 1;
                members = 1 << current_order;
            }
            index += 1;
        }
        // ___ Bootstrap memory for metadata ___
        let metadata_chunk_size = max!(bytes_needed, M);
        let _r = self
            .alloc(
                Layout::from_size_align(metadata_chunk_size, M)
                    .ok()
                    .expect("Woot ? At this point, all values are multiple of 2 !"),
            )
            .ok()
            .expect("Woot ? Already insuffisant memory ?!? That Buddy Allocator sucks !");
    }
    const fn alloc(&mut self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        match BuddySize::try_from(layout) {
            Ok(buddy_size) => match self.set_mark(buddy_size) {
                Ok(non_null) => Ok(non_null),
                // map_err(|e| ...) doesnot works in constant fn
                Err(e) => Err(format_error(e)),
            },
            Err(e) => Err(format_error(e)),
        }
    }
    fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        match BuddySize::try_from(layout) {
            Ok(buddy_size) => match Order::try_from((buddy_size, BuddySize::<M>(self.0.len()))) {
                Ok(order) => self.unset_mark(order, ptr).unwrap(),
                Err(e) => panic!("{}", e),
            },
            Err(e) => panic!("{}", e),
        }
    }
    const fn set_mark(&mut self, buddy_size: BuddySize<M>) -> Result<NonNull<[u8]>, &'static str> {
        let order = Order::try_from((buddy_size, BuddySize(self.0.len())))?.0;
        if order < self.0[FIRST_INDEX] {
            Err("Not enough room to swing a cat, a cat, the animal !")
        } else {
            let (mut index, mut current_order) = (FIRST_INDEX, 0); // Begin on index 1
            while current_order < order {
                // ___ Find the best fited block ___
                index = if self.0[2 * index] <= order {
                    2 * index // 2n --> binary heap
                } else {
                    2 * index + 1 // 2n + 1 --> binary heap
                };
                debug_assert!(
                    current_order < self.0[index],
                    "Woot ? That's definitively sucks"
                );
                current_order += 1;
            }
            // ___ Mark as occupied with 0x80 then mark order as 'max order' + 1 ___
            self.0[index] = 0x80
                + Order::try_from((BuddySize::<M>(M), BuddySize(self.0.len())))
                    .ok()
                    .expect("Woot ? Should be already checked !")
                    .0
                + 1;
            // ___ Calculate the pointer offset of the coresponding memory chunk ___
            let alloc_offset =
                self.0.len() / (1 << current_order) * (index & ((1 << current_order) - 1));
            // ___ Report changes on parents ___
            self.mark_parents(index, Order(current_order), Op::Allocate);
            Ok(NonNull::from(
                self.0
                    .get_mut(alloc_offset..alloc_offset + buddy_size.0)
                    .unwrap(),
            ))
        }
    }
    fn unset_mark(&mut self, order: Order, ptr: NonNull<u8>) -> Result<(), &'static str> {
        let alloc_offset = usize::from(ptr.addr()) - (self.0.get(0).unwrap() as *const u8 as usize);
        let start_idx = 1 << order.0;
        // Cast as u64 to avoid mul overflow on 32bits target
        #[cfg(target_pointer_width = "32")]
        let index = start_idx
            + (alloc_offset as u64 * (1 << order.0) as u64 / self.0.len() as u64) as usize;
        #[cfg(target_pointer_width = "64")]
        let index = start_idx + (alloc_offset * (1 << order.0) / self.0.len());
        if self.0[index] & 0x80 == 0 {
            Err("Double Free or corruption")
        } else {
            // ___ Mark as free, like original value ___
            self.0[index] = order.0;
            // ___ Report changes on parents ___
            self.mark_parents(index, order, Op::Deallocate);
            Ok(())
        }
    }
    #[inline(always)]
    const fn mark_parents(&mut self, mut index: usize, mut order: Order, op: Op) {
        while index > FIRST_INDEX {
            let parent = index / 2; // 1/2n --> binary heap
            let new_indice = match op {
                Op::Allocate => min!(self.0[2 * parent] & 0x7f, self.0[2 * parent + 1] & 0x7f),
                Op::Deallocate => {
                    if self.0[2 * parent] == order.0 && self.0[2 * parent + 1] == order.0 {
                        order.0 - 1
                    } else {
                        min!(self.0[2 * parent] & 0x7f, self.0[2 * parent + 1] & 0x7f)
                    }
                }
            };
            if self.0[parent] != new_indice {
                self.0[parent] = new_indice;
            } else {
                break; // Job finished
            }
            order.0 -= 1;
            index = parent;
        }
    }
}

impl<const M: usize> const TryFrom<(BuddySize<M>, BuddySize<M>)> for Order {
    type Error = &'static str;
    #[inline(always)]
    fn try_from(
        (buddy_size, max_buddy_size): (BuddySize<M>, BuddySize<M>),
    ) -> Result<Self, Self::Error> {
        // ___ Assuming in RELEASE profile that buddy sizes are pow of 2 ___
        debug_assert!(round_up_2(buddy_size.0 as u32) == buddy_size.0 as u32);
        debug_assert!(round_up_2(max_buddy_size.0 as u32) == max_buddy_size.0 as u32);
        let buddy_pow = trailing_zero_right(buddy_size.0 as u32);
        let space_pow = trailing_zero_right(max_buddy_size.0 as u32);
        if buddy_pow > space_pow {
            Err("the bigger buddy is too small for the requested size")
        } else {
            Ok(Order((space_pow - buddy_pow) as u8))
        }
    }
}

// TODO: Put MAX_SUPPORTED_ALIGN & MAX_BUDDY_SIZE into static string
impl<const M: usize> const TryFrom<Layout> for BuddySize<M> {
    type Error = &'static str;
    #[inline(always)]
    fn try_from(layout: Layout) -> Result<Self, Self::Error> {
        let size = max!(layout.size(), layout.align(), M);
        if size > MAX_BUDDY_SIZE {
            Err("Size must be lower or eq than {MAX_BUDDY_SIZE}")
        } else if layout.align() > MAX_SUPPORTED_ALIGN {
            Err("Alignement too big: MAX - {MAX_SUPPORTED_ALIGN}")
        } else {
            Ok(BuddySize(round_up_2(size as u32) as usize))
        }
    }
}

#[allow(unused_variables)]
const fn format_error(e: &'static str) -> AllocError {
    // NOTE: Problem to for using println in const FN
    // eprintln!("{}", e);
    AllocError
}
