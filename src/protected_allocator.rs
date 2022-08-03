//! Inner part of BuddyAllocator and StaticBuddyAllocator
/// Partie interne de l'allocateur.
pub struct ProtectedAllocator<'a, const M: usize>(pub &'a mut [u8]);

mod math;
use math::{round_up_2, trailing_zero_right};
#[macro_use]
mod macros;

use core::alloc::{AllocError, Layout};
use core::ptr::NonNull;

pub const MIN_BUDDY_SIZE: usize = 64;
pub const MAX_BUDDY_SIZE: usize = 0x2000_0000;
pub const MAX_SUPPORTED_ALIGN: usize = 4096;

const FIRST_INDEX: usize = 1;

// ___ Requested Buddy Size and Order with their TryFrom<_> boilerplates ___
#[derive(Debug, Copy, Clone)]
pub struct BuddySize<const M: usize>(pub usize);
#[derive(Debug, Copy, Clone)]
pub struct Order(pub u8);

enum Op {
    Allocate,
    Deallocate,
}

impl<'a, const M: usize> ProtectedAllocator<'a, M> {
    /// Initialisation, organise l'espace memoire en inscrivant les metadonnees necessaires.
    pub const fn init(&'a mut self) {
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
    /// Alloue un nouvel objet selon le layout et retourne son addresse.
    pub const fn alloc(&mut self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        match BuddySize::try_from(layout) {
            Ok(buddy_size) => match self.set_mark(buddy_size) {
                Ok(non_null) => Ok(non_null),
                // map_err(|e| ...) doesnot works in constant fn
                Err(e) => Err(format_error(e)),
            },
            Err(e) => Err(format_error(e)),
        }
    }
    /// Desalloue un objet prealablement alloue.
    pub fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) {
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
