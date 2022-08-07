mod math;
#[macro_use]
mod macros;

use math::{round_up_2, trailing_zero_right};

use core::alloc::Layout;
use core::ptr::NonNull;

/// Allowed size of the smallest buddy
pub const MIN_CELL_LEN: usize = 8; // arbitrary choice
/// TODO: The alignment constraint must be reviewed
pub const MAX_SUPPORTED_ALIGN: usize = 4096; // unix standard page size
/// Minimum number of buddy allowed
pub const MIN_BUDDY_NB: usize = 4; // arbitrary choice

const FIRST_INDEX: usize = 1; // index 0 is never used

/// Reference a valid Address Space
/// Inner part of BuddyAllocator and StaticBuddyAllocator
pub struct InnerAllocator<'a, const M: usize> {
    arena: &'a mut [u8],
    meta: &'a mut [u8],
    allocable_len: usize,
}

/// Use only for static allocation
#[repr(C, align(4096))]
pub struct StaticAddressSpace<const SIZE: usize, const M: usize>
where
    [(); SIZE / M * 2]:,
{
    arena: [u8; SIZE],
    meta: [u8; SIZE / M * 2],
}
impl<const SIZE: usize, const M: usize> StaticAddressSpace<SIZE, M>
where
    [(); SIZE / M * 2]:,
{
    /// Helper to create static const address space for allocations
    /// Be carefull, static chunks affect hugely the executable's size
    pub const fn new() -> Self {
        let mut meta: [u8; SIZE / M * 2] = [0; SIZE / M * 2];
        let arena: [u8; SIZE] = [0; SIZE];
        meta[0] = 0x42; // Tell metadata must be writed
        Self { arena, meta }
    }
}

/// Initialisation, organise l'espace memoire en inscrivant les metadonnees necessaires.
const fn check<const M: usize>(input: &mut [u8]) -> usize {
    // ___ MAX LEN OF ADDRESS SPACE IS CONSTRAINED BY USIZE BIT SCHEME, DEPENDS OF ARCH ___
    assert!(M >= MIN_CELL_LEN);
    // ___ Four Buddy minimum are allowed but is not optimal at all ___
    assert!(M <= usize::MAX / MIN_BUDDY_NB + 1);
    assert!(input.len() == usize::MAX || input.len() >= M * MIN_BUDDY_NB);
    assert!(input.len() == usize::MAX || round_up_2(input.len()) == input.len());
    assert!(round_up_2(M) == M);
    let current_align = if input.len() > MAX_SUPPORTED_ALIGN {
        MAX_SUPPORTED_ALIGN
    } else {
        input.len()
    };
    let ptr_offset = input.as_mut_ptr().align_offset(current_align);
    // IMPORTANT: On compile time with const fn feature, align_offset() doesn't works
    // and returns USIZE::MAX. Trust on you. Can't be sure...
    assert!(ptr_offset == 0 || ptr_offset == usize::MAX); // Check pointer alignement
    input.len() / M * 2
}

#[derive(Debug, Copy, Clone)]
pub struct BuddySize<const M: usize>(pub usize);
#[derive(Debug, Copy, Clone)]
pub struct Order(pub u8);

enum Op {
    Allocate,
    Deallocate,
}

impl<'a, const M: usize> InnerAllocator<'a, M> {
    /// TODO
    pub fn new_from_refs(ref_arena: &'a mut [u8], ref_meta: Option<&'a mut [u8]>) -> Self {
        let allocable_len = ref_arena.len();
        let metadata_size = check::<M>(ref_arena);
        let out = if let Some(meta) = ref_meta {
            Self {
                arena: ref_arena,
                meta,
                allocable_len,
            }
        } else {
            let (meta, arena) = ref_arena.split_at_mut(max!(metadata_size, M));
            Self {
                arena,
                meta,
                allocable_len,
            }
        };
        out.meta[0] = 0x42; // Tell metadata must be writed
        out
    }
    /// TODO
    pub const fn new_from_static<const SIZE: usize>(
        address_space: &'static mut StaticAddressSpace<SIZE, M>,
    ) -> Self
    where
        [(); SIZE / M * 2]:,
    {
        let allocable_len = address_space.arena.len();
        let out = Self {
            meta: &mut address_space.meta,
            arena: &mut address_space.arena,
            allocable_len,
        };
        let metadata_size = check::<M>(out.arena);
        assert!(metadata_size == out.meta.len());
        out
    }
    /// Check if metadata are already writed
    #[inline(always)]
    fn check_metadata(&mut self) {
        if self.meta[0] == 0x42 {
            self.write_metadata();
        }
        debug_assert!(self.meta[0] == 0xff);
    }
    fn write_metadata(&mut self) {
        let max_order = Order::try_from((BuddySize::<M>(M), BuddySize(self.allocable_len)))
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
        // it derives from <const SIZE: usize> so space.len(). So We have to hack the compiler to
        // allow 'infinite' eval limit. #![feature(const_eval_limit)] && #![const_eval_limit = "0"]
        // ___ Write original metadatas ___
        let (mut current_order, mut members, mut index) = (0, 2, 0);
        while index < bytes_needed {
            members -= 1;
            self.meta[index] = current_order;
            if members == 0 {
                current_order += 1;
                members = 1 << current_order;
            }
            index += 1;
        }
        // ___ Bootstrap memory for metadata ___
        if self.allocable_len != self.arena.len() {
            let metadata_chunk_size = max!(bytes_needed, M);
            let order = Order::try_from((
                BuddySize::<M>(metadata_chunk_size),
                BuddySize(self.allocable_len),
            ))
            .unwrap();
            self.set_mark(order)
                .ok()
                .expect("Woot ? Already insuffisant memory ?!? That Buddy Allocator sucks !");
        }
        self.meta[0] = 0xff; // Mark metadata done
    }
    /// TODO
    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> Result<NonNull<[u8]>, BuddyError> {
        self.check_metadata();
        let buddy_size = BuddySize::<M>::try_from(layout)?;
        let order = Order::try_from((buddy_size, BuddySize(self.allocable_len)))?;
        let index = self.set_mark(order)?;
        // ___ Calculate the pointer offset of the coresponding memory chunk ___
        let mut alloc_offset = self.allocable_len / (1 << order.0) * (index & ((1 << order.0) - 1));
        if self.allocable_len != self.arena.len() {
            // case metadata into allocated memory area
            alloc_offset -= self.meta.len();
        }
        // ___ Report changes on parents ___
        Ok(NonNull::from(
            self.arena
                .get_mut(alloc_offset..alloc_offset + buddy_size.0)
                .unwrap(),
        ))
    }
    /// TODO
    #[inline(always)]
    pub fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) -> Result<(), BuddyError> {
        self.check_metadata();
        let order = Order::try_from((
            BuddySize::try_from(layout)?,
            BuddySize::<M>(self.allocable_len),
        ))?;
        // L'arythmetique des pointeurs n'est pas possible dans une fonction constante.
        // ___ TODO: Explain that ! ___
        let alloc_offset = usize::from(ptr.addr())
            - if self.allocable_len != self.arena.len() {
                // case metadata into allocated memory area
                self.meta.get(0).unwrap()
            } else {
                // case metadata outside allocated memory area
                self.arena.get(0).unwrap()
            } as *const u8 as usize;
        let start_idx = 1 << order.0;
        // Cast as u64 to avoid mul overflow on 32bits target
        #[cfg(target_pointer_width = "32")]
        let index =
            start_idx + (alloc_offset as u64 * (1 << order.0) as u64 / space.len() as u64) as usize;
        // Cast as u128 to avoid mul overflow on 64bits target
        #[cfg(target_pointer_width = "64")]
        let index = start_idx
            + (alloc_offset as u128 * (1 << order.0) as u128 / self.allocable_len as u128) as usize;
        self.unset_mark(order, index)
    }
    /// TODO
    pub fn shrink(
        &mut self,
        _ptr: NonNull<u8>,
        _old_layout: Layout,
        _new_layout: Layout,
    ) -> Result<NonNull<[u8]>, BuddyError> {
        self.check_metadata();
        unimplemented!();
    }
    /// TODO
    pub fn grow(
        &mut self,
        _ptr: NonNull<u8>,
        _old_layout: Layout,
        _new_layout: Layout,
        _zeroed: bool,
    ) -> Result<NonNull<[u8]>, BuddyError> {
        self.check_metadata();
        unimplemented!();
    }
    /// TODO
    #[inline(always)]
    pub fn reserve(&mut self, _index: usize, _size: usize) -> Result<(), BuddyError> {
        self.check_metadata();
        unimplemented!();
    }
    /// TODO
    #[inline(always)]
    pub fn unreserve(&mut self, _index: usize) -> Result<(), BuddyError> {
        self.check_metadata();
        unimplemented!();
    }

    #[inline(always)]
    fn set_mark(&mut self, order: Order) -> Result<usize, BuddyError> {
        if order.0 < self.meta[FIRST_INDEX] {
            Err(BuddyError::NoMoreSpace)
        } else {
            let (mut index, mut current_order) = (FIRST_INDEX, 0); // Begin on index 1
            while current_order < order.0 {
                // ___ Find the best fited block ___
                index = if self.meta[2 * index] <= order.0 {
                    2 * index // 2n --> binary heap
                } else {
                    2 * index + 1 // 2n + 1 --> binary heap
                };
                debug_assert!(
                    current_order < self.meta[index],
                    "Woot ? That's definitively sucks"
                );
                current_order += 1;
            }
            // ___ Mark as occupied with 0x80 then mark order as 'max order' + 1 ___
            self.meta[index] = 0x80
                + Order::try_from((BuddySize::<M>(M), BuddySize(self.allocable_len)))
                    .ok()
                    .expect("Woot ? Should be already checked !")
                    .0
                + 1;
            self.modify_parents(index, Order(current_order), Op::Allocate);
            Ok(index)
        }
    }
    #[inline(always)]
    fn unset_mark(&mut self, order: Order, index: usize) -> Result<(), BuddyError> {
        if self.meta[index] & 0x80 == 0 {
            Err(BuddyError::DoubleFreeOrCorruption)
        } else {
            // ___ Mark as free, like original value ___
            self.meta[index] = order.0;
            // ___ Report changes on parents ___
            self.modify_parents(index, order, Op::Deallocate);
            Ok(())
        }
    }
    #[inline(always)]
    fn modify_parents(&mut self, mut index: usize, mut order: Order, op: Op) {
        while index > FIRST_INDEX {
            let parent = index / 2; // 1/2n --> binary heap
            let child_left = 2 * parent;
            let child_right = child_left + 1;
            let new_indice = match op {
                Op::Allocate => min!(self.meta[child_left] & 0x7f, self.meta[child_right] & 0x7f),
                Op::Deallocate => {
                    if self.meta[child_left] == order.0 && self.meta[child_right] == order.0 {
                        order.0 - 1
                    } else {
                        min!(self.meta[child_left] & 0x7f, self.meta[child_right] & 0x7f)
                    }
                }
            };
            if self.meta[parent] != new_indice {
                self.meta[parent] = new_indice;
            } else {
                break; // Job finished
            }
            order.0 -= 1;
            index = parent;
        }
    }
}

impl<const M: usize> TryFrom<(BuddySize<M>, BuddySize<M>)> for Order {
    type Error = BuddyError;
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
            Err(BuddyError::CannotFit)
        } else {
            Ok(Order((space_pow - buddy_pow) as u8))
        }
    }
}

impl<const M: usize> TryFrom<Layout> for BuddySize<M> {
    type Error = BuddyError;
    #[inline(always)]
    fn try_from(layout: Layout) -> Result<Self, Self::Error> {
        let size = max!(layout.size(), layout.align(), M);
        if size > usize::MAX / MIN_BUDDY_NB + 1 {
            Err(BuddyError::TooBigSize)
        } else if layout.align() > MAX_SUPPORTED_ALIGN {
            Err(BuddyError::TooBigAlignment)
        } else {
            Ok(BuddySize(round_up_2(size)))
        }
    }
}

/// Error types from Allocator
#[derive(Debug, Copy, Clone)]
pub enum BuddyError {
    /// Requested size cannot be allocated                                
    CannotFit,
    /// Alignment issue
    TooBigAlignment,
    /// Requested size cannot be allocated
    TooBigSize,
    /// Attempt to free when is impossible
    DoubleFreeOrCorruption,
    /// No more allocable space for requested size
    NoMoreSpace,
}

impl From<BuddyError> for &'static str {
    fn from(error: BuddyError) -> Self {
        use BuddyError::*;
        match error {
            CannotFit => "the bigger buddy is too small for the requested size",
            TooBigAlignment => "Alignement too big",
            TooBigSize => "Bad size",
            DoubleFreeOrCorruption => "Double Free or corruption",
            NoMoreSpace => "Not enough room to swing a cat, a cat, the animal !",
        }
    }
}
