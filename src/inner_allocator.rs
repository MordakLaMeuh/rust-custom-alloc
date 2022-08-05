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
pub struct AddressSpaceRef<'a, const M: usize>(&'a mut [u8]);

/// Use only for static allocation
#[repr(align(4096))]
pub struct StaticAddressSpace<const SIZE: usize, const M: usize>(pub [u8; SIZE]);

impl<const SIZE: usize, const M: usize> StaticAddressSpace<SIZE, M> {
    /// Helper to create static const address space for allocations
    /// Be carefull, static chunks affect hugely the executable's size
    pub const fn new() -> Self {
        let mut area: [u8; SIZE] = [0; SIZE];
        init::<M>(&mut area);
        StaticAddressSpace(area)
    }
}

impl<'a, const M: usize> const From<&'a mut [u8]> for AddressSpaceRef<'a, M> {
    fn from(space: &'a mut [u8]) -> Self {
        init::<M>(space);
        Self(space)
    }
}

impl<const SIZE: usize, const M: usize> const From<&'static mut StaticAddressSpace<SIZE, M>>
    for AddressSpaceRef<'static, M>
{
    fn from(static_address_space: &'static mut StaticAddressSpace<SIZE, M>) -> Self {
        Self(&mut static_address_space.0)
    }
}

/// Inner part of BuddyAllocator and StaticBuddyAllocator
pub struct InnerBuddy<'a, const M: usize>(AddressSpaceRef<'a, M>);

#[derive(Debug, Copy, Clone)]
pub struct BuddySize<const M: usize>(pub usize);
#[derive(Debug, Copy, Clone)]
pub struct Order(pub u8);

enum Op {
    Allocate,
    Deallocate,
}

impl<'a, const M: usize> InnerBuddy<'a, M> {
    /// TODO
    pub const fn new(address_space_ref: AddressSpaceRef<'a, M>) -> Self {
        Self(address_space_ref)
    }
    /// TODO
    #[inline(always)]
    pub fn alloc(&mut self, layout: Layout) -> Result<NonNull<[u8]>, BuddyError> {
        alloc::<M>(self.0 .0, layout)
    }
    /// TODO
    #[inline(always)]
    pub fn dealloc(&mut self, ptr: NonNull<u8>, layout: Layout) -> Result<(), BuddyError> {
        dealloc::<M>(self.0 .0, ptr, layout)
    }
    /// TODO
    #[inline(always)]
    pub fn reserve(&mut self, _index: usize, _size: usize) -> Result<(), BuddyError> {
        unimplemented!();
    }
    /// TODO
    #[inline(always)]
    pub fn unreserve(&mut self, _index: usize) -> Result<(), BuddyError> {
        unimplemented!();
    }
}

/// Initialisation, organise l'espace memoire en inscrivant les metadonnees necessaires.
const fn init<const M: usize>(space: &mut [u8]) {
    // ___ MAX LEN OF ADDRESS SPACE IS CONSTRAINED BY USIZE BIT SCHEME, DEPENDS OF ARCH ___
    assert!(M >= MIN_CELL_LEN);
    // ___ Four Buddy minimum are allowed but is not optimal at all ___
    assert!(M <= usize::MAX / MIN_BUDDY_NB + 1);
    assert!(space.len() == usize::MAX || space.len() >= M * MIN_BUDDY_NB);
    assert!(space.len() == usize::MAX || round_up_2(space.len()) == space.len());
    assert!(round_up_2(M) == M);
    let current_align = if space.len() > MAX_SUPPORTED_ALIGN {
        MAX_SUPPORTED_ALIGN
    } else {
        space.len()
    };
    let ptr_offset = space.as_mut_ptr().align_offset(current_align);
    // IMPORTANT: On compile time with const fn feature, align_offset() doesn't works
    // and returns USIZE::MAX. Trust on you. Can't be sure...
    assert!(ptr_offset == 0 || ptr_offset == usize::MAX); // Check pointer alignement
    let max_order = Order::try_from((BuddySize::<M>(M), BuddySize(space.len())))
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
        space[index] = current_order;
        if members == 0 {
            current_order += 1;
            members = 1 << current_order;
        }
        index += 1;
    }
    // ___ Bootstrap memory for metadata ___
    let metadata_chunk_size = max!(bytes_needed, M);
    alloc::<M>(
        space,
        Layout::from_size_align(metadata_chunk_size, M)
            .ok()
            .expect("Woot ? At this point, all values are multiple of 2 !"),
    )
    .ok()
    .expect("Woot ? Already insuffisant memory ?!? That Buddy Allocator sucks !");
}
/// Alloue un nouvel objet selon le layout et retourne son addresse.
#[inline(always)]
const fn alloc<const M: usize>(
    space: &mut [u8],
    layout: Layout,
) -> Result<NonNull<[u8]>, BuddyError> {
    set_mark::<M>(space, BuddySize::try_from(layout)?)
}
/// Desalloue un objet prealablement alloue.
#[inline(always)]
fn dealloc<const M: usize>(
    space: &mut [u8],
    ptr: NonNull<u8>,
    layout: Layout,
) -> Result<(), BuddyError> {
    let order = Order::try_from((BuddySize::try_from(layout)?, BuddySize::<M>(space.len())))?;
    // L'arythmetique des pointeurs n'est pas possible dans une fonction constante.
    let alloc_offset = usize::from(ptr.addr()) - (space.get(0).unwrap() as *const u8 as usize);
    let start_idx = 1 << order.0;
    // Cast as u64 to avoid mul overflow on 32bits target
    #[cfg(target_pointer_width = "32")]
    let index =
        start_idx + (alloc_offset as u64 * (1 << order.0) as u64 / space.len() as u64) as usize;
    // Cast as u128 to avoid mul overflow on 64bits target
    #[cfg(target_pointer_width = "64")]
    let index =
        start_idx + (alloc_offset as u128 * (1 << order.0) as u128 / space.len() as u128) as usize;
    unset_mark(space, order, index)
}
#[inline(always)]
const fn set_mark<const M: usize>(
    space: &mut [u8],
    buddy_size: BuddySize<M>,
) -> Result<NonNull<[u8]>, BuddyError> {
    let order = Order::try_from((buddy_size, BuddySize(space.len())))?.0;
    if order < space[FIRST_INDEX] {
        Err(BuddyError::NoMoreSpace)
    } else {
        let (mut index, mut current_order) = (FIRST_INDEX, 0); // Begin on index 1
        while current_order < order {
            // ___ Find the best fited block ___
            index = if space[2 * index] <= order {
                2 * index // 2n --> binary heap
            } else {
                2 * index + 1 // 2n + 1 --> binary heap
            };
            debug_assert!(
                current_order < space[index],
                "Woot ? That's definitively sucks"
            );
            current_order += 1;
        }
        // ___ Mark as occupied with 0x80 then mark order as 'max order' + 1 ___
        space[index] = 0x80
            + Order::try_from((BuddySize::<M>(M), BuddySize(space.len())))
                .ok()
                .expect("Woot ? Should be already checked !")
                .0
            + 1;
        // ___ Calculate the pointer offset of the coresponding memory chunk ___
        let alloc_offset =
            space.len() / (1 << current_order) * (index & ((1 << current_order) - 1));
        // ___ Report changes on parents ___
        modify_parents(space, index, Order(current_order), Op::Allocate);
        Ok(NonNull::from(
            space
                .get_mut(alloc_offset..alloc_offset + buddy_size.0)
                .unwrap(),
        ))
    }
}
#[inline(always)]
const fn unset_mark(space: &mut [u8], order: Order, index: usize) -> Result<(), BuddyError> {
    if space[index] & 0x80 == 0 {
        Err(BuddyError::DoubleFreeOrCorruption)
    } else {
        // ___ Mark as free, like original value ___
        space[index] = order.0;
        // ___ Report changes on parents ___
        modify_parents(space, index, order, Op::Deallocate);
        Ok(())
    }
}
#[inline(always)]
const fn modify_parents(space: &mut [u8], mut index: usize, mut order: Order, op: Op) {
    while index > FIRST_INDEX {
        let parent = index / 2; // 1/2n --> binary heap
        let child_left = 2 * parent;
        let child_right = child_left + 1;
        let new_indice = match op {
            Op::Allocate => min!(space[child_left] & 0x7f, space[child_right] & 0x7f),
            Op::Deallocate => {
                if space[child_left] == order.0 && space[child_right] == order.0 {
                    order.0 - 1
                } else {
                    min!(space[child_left] & 0x7f, space[child_right] & 0x7f)
                }
            }
        };
        if space[parent] != new_indice {
            space[parent] = new_indice;
        } else {
            break; // Job finished
        }
        order.0 -= 1;
        index = parent;
    }
}

impl<const M: usize> const TryFrom<(BuddySize<M>, BuddySize<M>)> for Order {
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

impl<const M: usize> const TryFrom<Layout> for BuddySize<M> {
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

impl const From<BuddyError> for &'static str {
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
