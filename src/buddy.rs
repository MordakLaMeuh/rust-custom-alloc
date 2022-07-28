use super::math::{round_up_2, trailing_zero_right};

use std::alloc::{AllocError, Layout};
use std::ptr::NonNull;
use std::sync::Mutex;

const MIN_BUDDY_SIZE: usize = 16;
const MAX_BUDDY_SIZE: usize = 0x8000_0000;
const MAX_SUPPORTED_ALIGN: usize = 4096;

/// Buddy Allocator
#[repr(C, align(16))]
pub struct BuddyAllocator<'a>(pub Mutex<ProtectedAllocator<'a>>);

impl<'a> BuddyAllocator<'a> {
    /// Create a new Buddy Allocator on a previous allocated block
    pub const fn new(address: &'a mut [u8]) -> Self {
        Self(Mutex::new(ProtectedAllocator::new(address)))
    }
}

// TODO. on final time, this struct must be placed into a choosen memory location
pub struct ProtectedAllocator<'a>(&'a mut [u8]);

macro_rules! max {
    ($x: expr) => ($x);
    ($x: expr, $($z: expr),+) => {{
        let y = max!($($z),*);
        if $x > y {
            $x
        } else {
            y
        }
    }}
}

impl<'a> ProtectedAllocator<'a> {
    const fn new(address: &'a mut [u8]) -> Self {
        assert!(address.len() <= MAX_BUDDY_SIZE); // Max limitation cf. 32b systems
        assert!(address.len() >= MIN_BUDDY_SIZE * 2);
        let space_rounded_up_2 = round_up_2(address.len() as u32);
        let space_order0_buddy = if space_rounded_up_2 == address.len() as u32 {
            address.len() as u32
        } else {
            space_rounded_up_2 >> 1
        };
        let current_align = if address.len() > MAX_SUPPORTED_ALIGN {
            MAX_SUPPORTED_ALIGN
        } else {
            address.len()
        };
        let ptr_offset = address.as_mut_ptr().align_offset(current_align);
        // On launch time with const fn feature, align_offset() doesn't works and returns USIZE::MAX
        // Trust on you
        assert!(ptr_offset == 0 || ptr_offset == usize::MAX); // Check pointer alignement
        let max_order = Order::try_from((
            BuddySize(MIN_BUDDY_SIZE as u32),
            BuddySize(space_order0_buddy),
        ))
        .ok()
        .expect("Woot ?");
        // Bits needed:           2^(order) * 4 (-2 [2 bits are unusable])
        // order 0.  2 bits       || xx
        // order 1.  6 bits       ||    + || ||
        // order 2. 14 bits       ||    + || || + || || || ||
        // order 3. 30 bits       ||    + || || + || || || || + || || || || || || || ||
        // [..]
        let bytes_needed = max!((2_u32.pow(max_order.0) * 4) / 8, MIN_BUDDY_SIZE as u32);
        // Bootstrap memory for metadata
        let mut this = Self(address.get_mut(0..space_order0_buddy as usize).unwrap());
        let _r = this
            .alloc(
                Layout::from_size_align(bytes_needed as usize, MIN_BUDDY_SIZE)
                    .ok()
                    .expect("Woot ?"),
            )
            .ok()
            .expect("Woot ?");
        this
    }
    #[inline(always)]
    const fn set_mark(&mut self, _order: Order) -> Result<NonNull<[u8]>, &'static str> {
        // Recurse descent into orders
        // if 0b00 | 0b01 go to the left
        // if 0b10 go the the right
        // if 0b00 and order is good, mark it
        // set 1 bit to parent
        // if 0b11 then recurse and set 0b11
        // dbg!(order);
        *self.0.get_mut(0).unwrap() = 42;
        Ok(NonNull::from(self.0.get_mut(..).unwrap()))
    }
    #[inline(always)]
    fn unset_mark(&mut self, order: Order, ptr: NonNull<u8>) -> Result<(), &'static str> {
        // check ptr align with order
        // let shr = 0 + (1 << order.0) + ptr(order);
        // verify mask 0b10 % ptr offset
        // set mask 0b00
        // verify nex buddy if 0b10
        // if 0b10 get 0 + (1 << (order.0 - 1)) + ptr(offset -> set as 0b01)
        // Verify if 0b11
        // if 0b00 get 0 + (1 << (order.0 - 1)) + ptr(offset -> set as 0b00)
        // Verify if 0b10 or (0b01) ???
        // END
        dbg!(order);
        dbg!(ptr);
        Ok(())
    }
}

// Requested Buddy Size and Order with their TryFrom<_> boilerplates
#[derive(Debug)]
struct BuddySize(u32);
#[derive(Debug)]
struct Order(u32);

impl const TryFrom<(BuddySize, BuddySize)> for Order {
    type Error = &'static str;
    fn try_from((buddy_size, max_buddy_size): (BuddySize, BuddySize)) -> Result<Self, Self::Error> {
        // Assuming in RELEASE profile that buddy sizes are pow of 2
        debug_assert!(round_up_2(buddy_size.0) == buddy_size.0);
        debug_assert!(round_up_2(max_buddy_size.0) == max_buddy_size.0);
        let buddy_pow = trailing_zero_right(buddy_size.0);
        let space_pow = trailing_zero_right(max_buddy_size.0);
        if buddy_pow > space_pow {
            Err("the bigger buddy is too small for the requested size")
        } else {
            Ok(Order(space_pow - buddy_pow))
        }
    }
}

// TODO: Put MAX_SUPPORTED_ALIGN & MAX_BUDDY_SIZE into static string
impl const TryFrom<Layout> for BuddySize {
    type Error = &'static str;
    fn try_from(layout: Layout) -> Result<Self, Self::Error> {
        let size = max!(layout.size(), layout.align(), MIN_BUDDY_SIZE as usize);
        match u32::try_from(size) {
            Ok(size) => {
                if size as usize > MAX_BUDDY_SIZE {
                    Err("Size must be lower or eq than {MAX_BUDDY_SIZE}")
                } else if layout.align() > MAX_SUPPORTED_ALIGN {
                    Err("Alignement too big: MAX - {MAX_SUPPORTED_ALIGN}")
                } else {
                    Ok(BuddySize(round_up_2(size)))
                }
            }
            Err(_e) => Err("Requested size must be fit into an u32"),
        }
    }
}

#[allow(unused_variables)]
const fn format_error(e: &'static str) -> AllocError {
    // TODO: Problem to for using println in const FN
    // eprintln!("{}", e);
    AllocError
}

impl<'a> ProtectedAllocator<'a> {
    #[inline(always)]
    pub const fn alloc(&mut self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        match BuddySize::try_from(layout) {
            Ok(buddy_size) => match Order::try_from((buddy_size, BuddySize(self.0.len() as u32))) {
                Ok(order) => match self.set_mark(order) {
                    Ok(non_null) => Ok(non_null),
                    // map_err(|e| ...) doesnot works in constant fn
                    Err(e) => {
                        format_error(e);
                        Err(AllocError)
                    }
                },
                Err(e) => Err(format_error(e)),
            },
            Err(e) => Err(format_error(e)),
        }
    }
    #[inline(always)]
    pub fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
        match BuddySize::try_from(layout) {
            Ok(buddy_size) => match Order::try_from((buddy_size, BuddySize(self.0.len() as u32))) {
                Ok(order) => self.unset_mark(order, ptr).unwrap(),
                Err(e) => panic!("{}", e),
            },
            Err(e) => panic!("{}", e),
        }
    }
}

impl<'a> Drop for BuddyAllocator<'a> {
    fn drop(&mut self) {
        println!("Drop Called");
    }
}

impl<'a> Drop for ProtectedAllocator<'a> {
    fn drop(&mut self) {}
}

#[cfg(test)]
mod test {
    use super::Layout;
    use super::{BuddySize, Order, ProtectedAllocator};
    use super::{MAX_BUDDY_SIZE, MAX_SUPPORTED_ALIGN, MIN_BUDDY_SIZE};

    const MEMORY_FIELD_SIZE: usize = 1024 * 1024 * 32;
    #[repr(align(4096))]
    struct MemoryField {
        pub array: [u8; MEMORY_FIELD_SIZE],
    }
    static mut MEMORY_FIELD: MemoryField = MemoryField {
        array: [0; MEMORY_FIELD_SIZE],
    };

    mod buddy_convert {
        use super::{BuddySize, Layout};
        #[test]
        fn normal() {
            [
                (4, 4, 16),
                (16, 16, 16),
                (4, 16, 16),
                (0, 1, 16),
                (0, 32, 32),
                (1, 1, 16),
                (1, 32, 32),
                (30, 16, 32),
                (17, 16, 32),
                (96, 16, 128),
                (513, 16, 1024),
                (5000, 64, 8192),
                (0x4000_0010, 4096, 0x8000_0000),
            ]
            .into_iter()
            .for_each(|(size, align, buddy_size)| {
                let layout = Layout::from_size_align(size, align)
                    .expect(format!("size {} align {}", size, align).as_str());
                assert_eq!(
                    BuddySize::try_from(layout).unwrap().0,
                    BuddySize(buddy_size).0,
                    "size {} align {} resulut {}",
                    size,
                    align,
                    buddy_size
                );
            });
        }
        #[should_panic]
        #[test]
        fn unsuported_align_request() {
            BuddySize::try_from(Layout::from_size_align(0x8000_0000, 8192).unwrap()).unwrap();
        }
        #[should_panic]
        #[test]
        fn unsuported_size_request() {
            BuddySize::try_from(Layout::from_size_align(0x9000_0000, 8192).unwrap()).unwrap();
        }
    }
    mod order_convert {
        use super::{BuddySize, Order};
        #[test]
        fn normal() {
            [
                (64, 64, 0),
                (32, 64, 1),
                (16, 64, 2),
                (16, 1024, 6),
                (32, 1024, 5),
                (1024, 4096, 2),
                (2048, 4096, 1),
                (4096, 4096, 0),
            ]
            .into_iter()
            .for_each(|(curr, max, order)| {
                assert_eq!(
                    Order::try_from((BuddySize(curr), BuddySize(max)))
                        .unwrap()
                        .0,
                    order,
                    "curr {} max {} order {}",
                    curr,
                    max,
                    order
                );
            });
        }
        #[should_panic]
        #[test]
        fn out_of_order() {
            Order::try_from((BuddySize(128), BuddySize(64))).unwrap();
        }
        #[should_panic]
        #[test]
        fn bad_buddy_size() {
            Order::try_from((BuddySize(32), BuddySize(120))).unwrap();
        }
    }
    mod constructor {
        use super::*;
        #[test]
        fn minimal_mem_block() {
            ProtectedAllocator::new(unsafe { &mut MEMORY_FIELD.array[..MIN_BUDDY_SIZE * 2] });
        }
        #[should_panic]
        #[test]
        fn too_small_mem_block() {
            ProtectedAllocator::new(unsafe { &mut MEMORY_FIELD.array[..MIN_BUDDY_SIZE] });
        }
        #[test]
        fn maximal_mem_block() {
            ProtectedAllocator::new(unsafe {
                std::slice::from_raw_parts_mut(MEMORY_FIELD.array.as_mut_ptr(), MAX_BUDDY_SIZE)
            });
        }
        #[should_panic]
        #[test]
        fn too_big_mem_block() {
            ProtectedAllocator::new(unsafe {
                std::slice::from_raw_parts_mut(
                    MEMORY_FIELD.array.as_mut_ptr(),
                    MAX_BUDDY_SIZE + 0x1000,
                )
            });
        }
        #[test]
        fn aligned_mem_block1() {
            ProtectedAllocator::new(unsafe {
                &mut MEMORY_FIELD.array[MIN_BUDDY_SIZE * 2..MIN_BUDDY_SIZE * 4]
            });
        }
        #[should_panic]
        #[test]
        fn bad_aligned_mem_block1() {
            ProtectedAllocator::new(unsafe { &mut MEMORY_FIELD.array[4..MIN_BUDDY_SIZE * 2 + 4] });
        }
        #[test]
        fn aligned_mem_block2() {
            ProtectedAllocator::new(unsafe {
                &mut MEMORY_FIELD.array[MIN_BUDDY_SIZE * 8..MIN_BUDDY_SIZE * 16]
            });
        }
        #[should_panic]
        #[test]
        fn bad_aligned_mem_block2() {
            ProtectedAllocator::new(unsafe {
                &mut MEMORY_FIELD.array[MIN_BUDDY_SIZE * 9..MIN_BUDDY_SIZE * 17]
            });
        }
        #[test]
        fn aligned_mem_block3() {
            ProtectedAllocator::new(unsafe {
                &mut MEMORY_FIELD.array[MAX_SUPPORTED_ALIGN..MAX_SUPPORTED_ALIGN * 17]
            });
        }
        #[should_panic]
        #[test]
        fn bad_aligned_mem_block3() {
            ProtectedAllocator::new(unsafe {
                &mut MEMORY_FIELD.array[(MAX_SUPPORTED_ALIGN / 2)
                    ..(MAX_SUPPORTED_ALIGN * 16) + (MAX_SUPPORTED_ALIGN / 2)]
            });
        }
    }
}