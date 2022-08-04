mod math;
use math::{round_up_2, trailing_zero_right};
#[macro_use]
mod macros;

use core::alloc::Layout;
use core::ptr::NonNull;

/// Allowed size of the smallest buddy
pub const MIN_CELL_LEN: usize = 8; // arbitrary choice
/// TODO: The alignment constraint must be reviewed
pub const MAX_SUPPORTED_ALIGN: usize = 4096; // unix standard page size
/// Minimum number of buddy allowed
pub const MIN_BUDDY_NB: usize = 4; // arbitrary choice

const FIRST_INDEX: usize = 1; // index 0 is never used

/// Inner part of BuddyAllocator and StaticBuddyAllocator
pub struct ProtectedAllocator<'a, const M: usize>(pub &'a mut [u8]);

#[derive(Debug, Copy, Clone)]
pub struct BuddySize<const M: usize>(pub usize);
#[derive(Debug, Copy, Clone)]
pub struct Order(pub u8);

#[derive(Debug)]
pub struct Error(&'static str);

enum Op {
    Allocate,
    Deallocate,
}

impl<'a, const M: usize> ProtectedAllocator<'a, M> {
    /// Initialisation, organise l'espace memoire en inscrivant les metadonnees necessaires.
    pub const fn init(&'a mut self) {
        // ___ MAX LEN OF ADDRESS SPACE IS CONSTRAINED BY USIZE BIT SCHEME, DEPENDS OF ARCH ___
        assert!(M >= MIN_CELL_LEN);
        // ___ Four Buddy minimum are allowed but is not optimal at all ___
        assert!(M <= usize::MAX / MIN_BUDDY_NB + 1);
        assert!(self.0.len() == usize::MAX || self.0.len() >= M * MIN_BUDDY_NB);
        assert!(self.0.len() == usize::MAX || round_up_2(self.0.len()) == self.0.len());
        assert!(round_up_2(M) == M);
        let current_align = if self.0.len() > MAX_SUPPORTED_ALIGN {
            MAX_SUPPORTED_ALIGN
        } else {
            self.0.len()
        };
        let ptr_offset = self.0.as_mut_ptr().align_offset(current_align);
        // IMPORTANT: On compile time with const fn feature, align_offset() doesn't works
        // and returns USIZE::MAX. Trust on you. Can't be sure...
        assert!(ptr_offset == 0 || ptr_offset == usize::MAX); // Check pointer alignement
        let max_order = Order::try_from((BuddySize::<M>(M), BuddySize(self.0.len())))
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
    pub const fn alloc(&mut self, layout: Layout) -> Result<NonNull<[u8]>, Error> {
        self.set_mark(BuddySize::try_from(layout)?)
    }
    /// Desalloue un objet prealablement alloue.
    pub fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) -> Result<(), Error> {
        let order = Order::try_from((BuddySize::try_from(layout)?, BuddySize::<M>(self.0.len())))?;
        // L'arythmetique des pointeurs n'est pas possible dans une fonction constante.
        let alloc_offset = usize::from(ptr.addr()) - (self.0.get(0).unwrap() as *const u8 as usize);
        let start_idx = 1 << order.0;
        // Cast as u64 to avoid mul overflow on 32bits target
        #[cfg(target_pointer_width = "32")]
        let index = start_idx
            + (alloc_offset as u64 * (1 << order.0) as u64 / self.0.len() as u64) as usize;
        // Cast as u128 to avoid mul overflow on 64bits target
        #[cfg(target_pointer_width = "64")]
        let index = start_idx
            + (alloc_offset as u128 * (1 << order.0) as u128 / self.0.len() as u128) as usize;
        self.unset_mark(order, index)
    }
    const fn set_mark(&mut self, buddy_size: BuddySize<M>) -> Result<NonNull<[u8]>, Error> {
        let order = Order::try_from((buddy_size, BuddySize(self.0.len())))?.0;
        if order < self.0[FIRST_INDEX] {
            Err(Error("Not enough room to swing a cat, a cat, the animal !"))
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
            self.modify_parents(index, Order(current_order), Op::Allocate);
            Ok(NonNull::from(
                self.0
                    .get_mut(alloc_offset..alloc_offset + buddy_size.0)
                    .unwrap(),
            ))
        }
    }
    const fn unset_mark(&mut self, order: Order, index: usize) -> Result<(), Error> {
        if self.0[index] & 0x80 == 0 {
            Err(Error("Double Free or corruption"))
        } else {
            // ___ Mark as free, like original value ___
            self.0[index] = order.0;
            // ___ Report changes on parents ___
            self.modify_parents(index, order, Op::Deallocate);
            Ok(())
        }
    }
    #[inline(always)]
    const fn modify_parents(&mut self, mut index: usize, mut order: Order, op: Op) {
        while index > FIRST_INDEX {
            let parent = index / 2; // 1/2n --> binary heap
            let child_left = 2 * parent;
            let child_right = child_left + 1;
            let new_indice = match op {
                Op::Allocate => min!(self.0[child_left] & 0x7f, self.0[child_right] & 0x7f),
                Op::Deallocate => {
                    if self.0[child_left] == order.0 && self.0[child_right] == order.0 {
                        order.0 - 1
                    } else {
                        min!(self.0[child_left] & 0x7f, self.0[child_right] & 0x7f)
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
    type Error = Error;
    #[inline(always)]
    fn try_from(
        (buddy_size, max_buddy_size): (BuddySize<M>, BuddySize<M>),
    ) -> Result<Self, Self::Error> {
        // ___ Assuming in RELEASE profile that buddy sizes are pow of 2 ___
        debug_assert!(round_up_2(buddy_size.0) == buddy_size.0);
        debug_assert!(
            max_buddy_size.0 == usize::MAX || round_up_2(max_buddy_size.0) == max_buddy_size.0
        );
        let buddy_pow = trailing_zero_right(buddy_size.0);
        #[cfg(target_pointer_width = "32")]
        let space_pow = if max_buddy_size.0 == usize::MAX {
            32
        } else {
            trailing_zero_right(max_buddy_size.0)
        };
        #[cfg(target_pointer_width = "64")]
        let space_pow = if max_buddy_size.0 == usize::MAX {
            64
        } else {
            trailing_zero_right(max_buddy_size.0)
        };
        if buddy_pow > space_pow {
            Err(Error(
                "the bigger buddy is too small for the requested size",
            ))
        } else {
            Ok(Order((space_pow - buddy_pow) as u8))
        }
    }
}

impl<const M: usize> const TryFrom<Layout> for BuddySize<M> {
    type Error = Error;
    #[inline(always)]
    fn try_from(layout: Layout) -> Result<Self, Self::Error> {
        let size = max!(layout.size(), layout.align(), M);
        if size > usize::MAX / MIN_BUDDY_NB + 1 {
            Err(Error("Bad size"))
        } else if layout.align() > MAX_SUPPORTED_ALIGN {
            Err(Error("Alignement too big"))
        } else {
            Ok(BuddySize(round_up_2(size)))
        }
    }
}
