//! Custom Allocator based on buddy System
#![deny(missing_docs)]
#![feature(allocator_api)]
#![feature(unchecked_math)]
#![feature(const_align_offset)]
#![feature(const_mut_refs)]

mod math;
use math::{round_up_2, trailing_zero_right};

use std::alloc::{handle_alloc_error, AllocError, Allocator, GlobalAlloc, Layout};
use std::ptr::{null_mut, NonNull};
use std::sync::Mutex;

// TODO: Creation must be done when ProtectedAllocator::new is called
const MEMORY_FIELD_SIZE: usize = 1024 * 1024 * 32;
#[repr(align(4096))]
struct MemoryField {
    pub array: [u8; MEMORY_FIELD_SIZE],
}
static mut MEMORY_FIELD: MemoryField = MemoryField {
    array: [0; MEMORY_FIELD_SIZE],
};

const MIN_BUDDY_SIZE: u32 = 16;
const MAX_BUDDY_SIZE: u32 = 0x8000_0000;
const MAX_SUPPORTED_ALIGN: u32 = 4096;

/// Buddy Allocator
pub struct BuddyAllocator {
    allocator: Mutex<ProtectedAllocator>,
}

impl BuddyAllocator {
    /// Create a new Buddy Allocator on a previous allocated block
    pub const fn new(address: *mut u8, space: u32) -> Self {
        Self {
            allocator: Mutex::new(ProtectedAllocator::new(address, space)),
        }
    }
}

// TODO. on final time, this struct must be placed into a choosen memory location
#[repr(C, align(4096))]
struct ProtectedAllocator {
    address: *mut u8,
    space: u32,
}

impl ProtectedAllocator {
    const fn new(address: *mut u8, space: u32) -> Self {
        assert!(space <= MAX_BUDDY_SIZE); // Max limitation cf. 32b systems
        assert!(space >= MIN_BUDDY_SIZE); // Min limitation cf. 32b systems
        let space_rounded_up_2 = round_up_2(space);
        let space_order0_buddy = if space_rounded_up_2 == space {
            space
        } else {
            space_rounded_up_2 >> 1
        };
        let min_aligned = if space > MAX_SUPPORTED_ALIGN {
            MAX_SUPPORTED_ALIGN
        } else {
            space
        };
        let ptr_offset = address.align_offset(min_aligned as usize);
        // On launch time with const fn feature, align_offset() doesn't works and returns USIZE::MAX
        // Trust on you
        assert!(ptr_offset == 0 || ptr_offset == usize::MAX); // Check pointer alignement
        Self {
            address,
            space: space_order0_buddy,
        }
    }
    #[inline(always)]
    fn set_mark(&mut self, order: Order) -> *mut u8 {
        dbg!(order);
        dbg!(self.address)
    }
    #[inline(always)]
    fn unset_mark(&mut self, order: Order, ptr: *mut u8) -> Result<(), &'static str> {
        dbg!(order);
        dbg!(ptr);
        Ok(())
    }
}

// Requested Buddy Size and Order with their TryFrom<_> boilerplates
#[derive(Debug, Clone)]
struct BuddySize(u32);
#[derive(Debug, Clone)]
struct Order(u32);

impl TryFrom<(BuddySize, u32)> for Order {
    type Error = &'static str;
    fn try_from((buddy_size, space): (BuddySize, u32)) -> Result<Self, Self::Error> {
        dbg!(&buddy_size);
        let buddy_pow = trailing_zero_right(buddy_size.0);
        let space_pow = trailing_zero_right(space);
        if buddy_pow > space_pow {
            Err("the bigger buddy is to small for the requested size")
        } else {
            Ok(Order(space_pow - buddy_pow))
        }
    }
}

// TODO: Put MAX_SUPPORTED_ALIGN & MAX_BUDDY_SIZE into static string
impl TryFrom<Layout> for BuddySize {
    type Error = &'static str;
    fn try_from(layout: Layout) -> Result<Self, Self::Error> {
        let size = usize::max(
            usize::max(layout.size(), layout.align()),
            MIN_BUDDY_SIZE as usize,
        );
        match u32::try_from(size) {
            Ok(size) => {
                if size > MAX_BUDDY_SIZE {
                    Err("Size must be lower or eq than {MAX_BUDDY_SIZE}")
                } else if layout.align() as u32 > MAX_SUPPORTED_ALIGN {
                    Err("Alignement too big: MAX - {MAX_SUPPORTED_ALIGN}")
                } else {
                    Ok(BuddySize(round_up_2(size)))
                }
            }
            Err(_e) => Err("Requested size must be fit into an u32"),
        }
    }
}

impl ProtectedAllocator {
    #[inline(always)]
    fn alloc(&mut self, layout: Layout) -> *mut u8 {
        match BuddySize::try_from(layout) {
            Ok(buddy_size) => match Order::try_from((buddy_size, self.space)) {
                Ok(order) => self.set_mark(order),
                Err(e) => {
                    eprintln!("{}", e);
                    null_mut()
                }
            },
            Err(e) => {
                eprintln!("{}", e);
                null_mut()
            }
        }
    }
    #[inline(always)]
    fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        match BuddySize::try_from(layout) {
            Ok(buddy_size) => match Order::try_from((buddy_size, self.space)) {
                Ok(order) => self.unset_mark(order, ptr).unwrap(),
                Err(e) => panic!("{}", e),
            },
            Err(e) => panic!("{}", e),
        }
    }
}

impl Drop for BuddyAllocator {
    fn drop(&mut self) {}
}

impl Drop for ProtectedAllocator {
    fn drop(&mut self) {}
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
static ALLOCATOR: BuddyAllocator =
    BuddyAllocator::new(unsafe { MEMORY_FIELD.array.as_mut_ptr() }, unsafe {
        MEMORY_FIELD.array.len() as u32
    });

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
