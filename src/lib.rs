//! Custom Allocator based on buddy System
#![deny(missing_docs)]
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
mod macros;

#[cfg(test)]
mod random;

// TODO: Find a solution with no_std
use core::mem::forget;
use core::ptr::NonNull;
use std::alloc::{handle_alloc_error, AllocError, Allocator, GlobalAlloc, Layout};
use std::sync::{Arc, Mutex};
// TODO: Create good documentations
// TODO: Draw nodes to explain the Buddy research update tree

unsafe impl<'a, const M: usize> Allocator for BuddyAllocator<'a, M> {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        // println!("[Alloc size: {} align: {}]", layout.size(), layout.align());
        // self.debug();
        let out = self.0.lock().unwrap().alloc(layout);
        // self.debug();
        out
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        // println!(
        //     "[Free size: {} align: {} ptr: {:?}]",
        //     layout.size(),
        //     layout.align(),
        //     ptr
        // );
        // self.debug();
        self.0.lock().unwrap().dealloc(ptr, layout);
        // self.debug();
    }
}

unsafe impl<'a, const M: usize> GlobalAlloc for StaticBuddyAllocator<'a, M> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.0.lock().unwrap().alloc(layout) {
            Ok(non_null) => non_null.as_mut_ptr(),
            Err(_) => handle_alloc_error(layout),
        }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.0
            .lock()
            .unwrap()
            .dealloc(NonNull::new(ptr).unwrap(), layout);
    }
}

const MIN_BUDDY_SIZE: usize = 64;
const MAX_BUDDY_SIZE: usize = 0x2000_0000;
const MAX_SUPPORTED_ALIGN: usize = 4096;

const FIRST_INDEX: usize = 1;

/// Buddy Allocator
#[repr(C, align(16))]
pub struct StaticBuddyAllocator<'a, const M: usize = MIN_BUDDY_SIZE>(
    Mutex<ProtectedAllocator<'a, M>>,
);

/// Buddy Allocator
#[derive(Clone)]
#[repr(C, align(16))]
pub struct BuddyAllocator<'a, const M: usize = MIN_BUDDY_SIZE>(
    Arc<Mutex<ProtectedAllocator<'a, M>>>,
);

struct ProtectedAllocator<'a, const M: usize>(pub &'a mut [u8]);

// ___ Requested Buddy Size and Order with their TryFrom<_> boilerplates ___
#[derive(Debug, Copy, Clone)]
struct BuddySize<const M: usize>(usize);
#[derive(Debug, Copy, Clone)]
struct Order(u8);

/// Use only for static allocation
#[repr(align(4096))]
pub struct StaticChunk<const SIZE: usize, const M: usize>(pub [u8; SIZE]);

impl<'a, const M: usize> BuddyAllocator<'a, M> {
    /// Create a new Buddy Allocator
    pub fn new(address: &'a mut [u8]) -> Self {
        Self(Arc::new(Mutex::new(ProtectedAllocator::new(address))))
    }
    /// Used only for debug purposes
    #[allow(dead_code)]
    fn debug(&self) {
        for (i, v) in self.0.lock().unwrap().0.iter().enumerate() {
            print!("{:02x} ", v);
            if i != 0 && (i + 1) % 32 == 0 {
                println!();
            }
        }
        println!();
    }
}

/// Helper to create static const chunks for allocations
/// Be carefull, static chunks affect hugely the executable's size
pub const fn create_static_chunk<const SIZE: usize, const M: usize>() -> StaticChunk<SIZE, M> {
    let mut area: [u8; SIZE] = [0; SIZE];
    forget(StaticBuddyAllocator::<M>::new(&mut area));
    StaticChunk(area)
}

impl<'a, const M: usize> StaticBuddyAllocator<'a, M> {
    // ___ Create a new Buddy Allocator on a previous allocated block ___
    const fn new(address: &'a mut [u8]) -> Self {
        Self(Mutex::new(ProtectedAllocator::new(address)))
    }
    /// Attach a previously allocated chunk generated by create_static_memory_area()
    pub const fn attach_static_chunk<const SIZE: usize>(
        address: &'static mut StaticChunk<SIZE, M>,
    ) -> Self {
        Self(Mutex::new(ProtectedAllocator::attach_static_chunk(address)))
    }
}

enum Op {
    Allocate,
    Deallocate,
}

impl<'a, const M: usize> ProtectedAllocator<'a, M> {
    const fn new(address: &'a mut [u8]) -> Self {
        assert!(M >= MIN_BUDDY_SIZE);
        assert!(M <= MAX_BUDDY_SIZE);
        assert!(round_up_2(M as u32) as usize == M);
        assert!(address.len() <= MAX_BUDDY_SIZE);
        assert!(address.len() >= M * 2);
        let space_rounded_up_2 = round_up_2(address.len() as u32) as usize;
        let space_order0_buddy = if space_rounded_up_2 == address.len() {
            address.len()
        } else {
            space_rounded_up_2 >> 1
        };
        let current_align = if address.len() > MAX_SUPPORTED_ALIGN {
            MAX_SUPPORTED_ALIGN
        } else {
            address.len()
        };
        let ptr_offset = address.as_mut_ptr().align_offset(current_align);
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
            address[index] = current_order;
            if members == 0 {
                current_order += 1;
                members = 1 << current_order;
            }
            index += 1;
        }
        // ___ Bootstrap memory for metadata ___
        let mut this = Self(address.get_mut(0..space_order0_buddy).unwrap());
        let metadata_chunk_size = max!(bytes_needed, M);
        let _r = this
            .alloc(
                Layout::from_size_align(metadata_chunk_size, M)
                    .ok()
                    .expect("Woot ? At this point, all values are multiple of 2 !"),
            )
            .ok()
            .expect("Woot ? Already insuffisant memory ?!? That Buddy Allocator sucks !");
        this
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
    const fn attach_static_chunk<const SIZE: usize>(chunk: &'a mut StaticChunk<SIZE, M>) -> Self {
        Self(&mut chunk.0)
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

#[cfg(test)]
mod test {
    use super::random::{srand_init, Rand};
    use super::Layout;
    use super::{BuddyAllocator, BuddySize, Order, ProtectedAllocator};
    use super::{MAX_BUDDY_SIZE, MAX_SUPPORTED_ALIGN, MIN_BUDDY_SIZE};

    const MEMORY_FIELD_SIZE: usize = MAX_BUDDY_SIZE;
    #[repr(align(4096))]
    struct MemoryField {
        pub array: [u8; MEMORY_FIELD_SIZE],
    }
    static mut MEMORY_FIELD: MemoryField = MemoryField {
        array: [0; MEMORY_FIELD_SIZE],
    };

    mod allocator {
        use super::BuddyAllocator;
        use super::MIN_BUDDY_SIZE;
        use super::{srand_init, Rand};
        #[test]
        fn fill_and_empty() {
            #[repr(align(4096))]
            struct MemChunk([u8; 256]);
            let mut chunk = MemChunk([0; 256]);
            let alloc: BuddyAllocator<64> = BuddyAllocator::new(&mut chunk.0);

            let mut v = Vec::new();
            for _ in 0..3 {
                v.push(Box::try_new_in([0xaa_u8; 64], &alloc).expect("AError"));
            }
            let b = Box::try_new_in([0xaa_u8; 64], &alloc);
            if let Ok(_) = b {
                panic!("Should not allocate again");
            }
            drop(v);
            let b = Box::try_new_in([0xaa_u8; 128], &alloc);
            if let Err(_) = &b {
                panic!("Allocation error");
            }
        }
        #[test]
        fn minimal() {
            #[repr(align(4096))]
            struct MemChunk([u8; MIN_BUDDY_SIZE * 2]);
            let mut chunk = MemChunk([0; MIN_BUDDY_SIZE * 2]);
            let alloc: BuddyAllocator<64> = BuddyAllocator::new(&mut chunk.0);
            let b = Box::try_new_in([0xaa_u8; 64], &alloc);
            if let Err(_) = &b {
                panic!("Should be done");
            }
            let g = Box::try_new_in([0xbb_u8; 64], &alloc);
            if let Ok(_v) = &g {
                panic!("Should Fail");
            }
        }
        #[test]
        fn minimal_with_other_generic() {
            #[repr(align(4096))]
            struct MemChunk([u8; MIN_BUDDY_SIZE * 4]);
            let mut chunk = MemChunk([0; MIN_BUDDY_SIZE * 4]);
            let alloc = BuddyAllocator::<128>::new(&mut chunk.0);
            let b = Box::try_new_in([0xaa_u8; MIN_BUDDY_SIZE * 2], &alloc);
            if let Err(_) = &b {
                panic!("Should be done");
            }
            let g = Box::try_new_in([0xbb_u8; MIN_BUDDY_SIZE * 2], &alloc);
            if let Ok(_v) = &g {
                panic!("Should Fail");
            }
        }
        // ___ This test is the most important ___
        const NB_TESTS: usize = 4096;
        const MO: usize = 1024 * 1024;
        const CHUNK_SIZE: usize = MO * 16;
        #[repr(align(4096))]
        struct MemChunk([u8; CHUNK_SIZE]);
        static mut CHUNK: MemChunk = MemChunk([0; CHUNK_SIZE]);
        // TODO: Take custom rand.rs instead: dependencies...
        struct Entry<'a> {
            content: Vec<u8, BuddyAllocator<'a>>,
            data: u8,
        }
        const ALLOC_SIZE: &[usize] = &[64, 128, 256, 512, 1024, 2048, 4096];
        fn repeat_test(alloc: BuddyAllocator) {
            let mut v = Vec::new();
            for _ in 0..NB_TESTS {
                match bool::srand(true) {
                    true if v.len() > 200 => {
                        let entry: Entry = v.remove(usize::srand(v.len() - 1));
                        for s in entry.content.iter() {
                            if *s != entry.data {
                                panic!("Corrupted Memory...");
                            }
                        }
                    }
                    _ => {
                        let size = ALLOC_SIZE[usize::srand(ALLOC_SIZE.len() - 1)];
                        let data = u8::srand(u8::MAX);
                        let mut content = Vec::new_in(alloc.clone());
                        for _ in 0..size {
                            content.push(data);
                        }
                        v.push(Entry { content, data });
                    }
                }
            }
            drop(v); // Flush all the alocator content
        }
        fn final_test(alloc: BuddyAllocator) {
            let mut v = Vec::new_in(alloc.clone());
            v.try_reserve(MO * 6).unwrap(); // Take the right buffy order 1 inside the allocator
            for _ in 0..(MO * 6) {
                v.push(42_u8);
            }
            let out = v.try_reserve(MO * 6); // The allocator cannot handle that
            if let Ok(_) = &out {
                panic!("This allocation is impossible");
            }
        }
        #[test]
        fn memory_sodomizer() {
            srand_init(42);
            for _ in 0..4 {
                let alloc: BuddyAllocator<64> = BuddyAllocator::new(unsafe { &mut CHUNK.0 });
                repeat_test(alloc.clone());
                final_test(alloc.clone());
            }
        }
        #[test]
        fn memory_sodomizer_multithreaded() {
            srand_init(42);
            // TODO: Not using libc crate for tests... need install gcc multilib...
            let chunk = unsafe { libc::memalign(4096, CHUNK_SIZE) as *mut u8 };
            let slice = unsafe { std::slice::from_raw_parts_mut(chunk, CHUNK_SIZE) };
            // thread::spawn can only take static reference so force the compiler by
            // transmuting to cast reference as static. And ensure you manually that
            // the object will continue to live.
            let static_slice =
                unsafe { std::mem::transmute::<&mut [u8], &'static mut [u8]>(slice) };
            let alloc: BuddyAllocator<64> = BuddyAllocator::new(static_slice);
            let mut thread_list = Vec::new();
            for _ in 0..4 {
                let clone = alloc.clone();
                thread_list.push(std::thread::spawn(move || {
                    repeat_test(clone.clone());
                }));
            }
            for thread in thread_list.into_iter() {
                drop(thread.join());
            }
            final_test(alloc.clone());
            // drop(v); // IMPORTANT: The last allocated object must be droped BEFORE freeing memory
            unsafe {
                libc::free(chunk as *mut _);
            }
        }
    }
    mod buddy_convert {
        use super::{BuddySize, Layout};
        use super::{MAX_BUDDY_SIZE, MAX_SUPPORTED_ALIGN, MIN_BUDDY_SIZE};
        #[test]
        fn normal() {
            [
                (MIN_BUDDY_SIZE / 4, MIN_BUDDY_SIZE / 4, MIN_BUDDY_SIZE),
                (MIN_BUDDY_SIZE, MIN_BUDDY_SIZE, MIN_BUDDY_SIZE),
                (MIN_BUDDY_SIZE / 4, MIN_BUDDY_SIZE, MIN_BUDDY_SIZE),
                (0, MIN_BUDDY_SIZE, MIN_BUDDY_SIZE),
                (0, MIN_BUDDY_SIZE * 2, MIN_BUDDY_SIZE * 2),
                (1, 1, MIN_BUDDY_SIZE),
                (1, MIN_BUDDY_SIZE * 2, MIN_BUDDY_SIZE * 2),
                (MIN_BUDDY_SIZE * 2 - 2, MIN_BUDDY_SIZE, MIN_BUDDY_SIZE * 2),
                (MIN_BUDDY_SIZE + 1, MIN_BUDDY_SIZE, MIN_BUDDY_SIZE * 2),
                (MIN_BUDDY_SIZE * 8, MIN_BUDDY_SIZE, MIN_BUDDY_SIZE * 8),
                (MIN_BUDDY_SIZE * 32 + 1, MIN_BUDDY_SIZE, MIN_BUDDY_SIZE * 64),
                (MIN_BUDDY_SIZE * 257, MIN_BUDDY_SIZE, MIN_BUDDY_SIZE * 512),
                (MAX_BUDDY_SIZE / 2 + 1, MAX_SUPPORTED_ALIGN, MAX_BUDDY_SIZE),
            ]
            .into_iter()
            .for_each(|(size, align, buddy_size)| {
                let layout = Layout::from_size_align(size, align)
                    .expect(format!("size {} align {}", size, align).as_str());
                assert_eq!(
                    BuddySize::<MIN_BUDDY_SIZE>::try_from(layout).unwrap().0,
                    BuddySize::<MIN_BUDDY_SIZE>(buddy_size.try_into().unwrap()).0,
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
            BuddySize::<MIN_BUDDY_SIZE>::try_from(
                Layout::from_size_align(MAX_BUDDY_SIZE, MAX_SUPPORTED_ALIGN * 2).unwrap(),
            )
            .unwrap();
        }
        #[should_panic]
        #[test]
        fn unsuported_size_request() {
            BuddySize::<MIN_BUDDY_SIZE>::try_from(
                Layout::from_size_align(MAX_BUDDY_SIZE + 0x1000_0000, MAX_SUPPORTED_ALIGN).unwrap(),
            )
            .unwrap();
        }
    }
    mod order_convert {
        use super::MIN_BUDDY_SIZE;
        use super::{BuddySize, Order};
        #[test]
        fn normal() {
            [
                (MIN_BUDDY_SIZE, MIN_BUDDY_SIZE, 0),
                (MIN_BUDDY_SIZE * 2, MIN_BUDDY_SIZE * 4, 1),
                (MIN_BUDDY_SIZE * 4, MIN_BUDDY_SIZE * 16, 2),
                (MIN_BUDDY_SIZE, MIN_BUDDY_SIZE * 64, 6),
                (MIN_BUDDY_SIZE * 2, MIN_BUDDY_SIZE * 64, 5),
                (MIN_BUDDY_SIZE * 64, MIN_BUDDY_SIZE * 256, 2),
                (MIN_BUDDY_SIZE * 128, MIN_BUDDY_SIZE * 256, 1),
                (MIN_BUDDY_SIZE * 256, MIN_BUDDY_SIZE * 256, 0),
            ]
            .into_iter()
            .for_each(|(curr, max, order)| {
                assert_eq!(
                    Order::try_from((
                        BuddySize::<MIN_BUDDY_SIZE>(curr),
                        BuddySize::<MIN_BUDDY_SIZE>(max)
                    ))
                    .expect(&format!("curr {} max {}", curr, max))
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
            Order::try_from((
                BuddySize::<MIN_BUDDY_SIZE>(MIN_BUDDY_SIZE * 8),
                BuddySize::<MIN_BUDDY_SIZE>(MIN_BUDDY_SIZE * 4),
            ))
            .unwrap();
        }
        #[should_panic]
        #[test]
        fn bad_buddy_size() {
            Order::try_from((
                BuddySize::<MIN_BUDDY_SIZE>(MIN_BUDDY_SIZE * 2),
                BuddySize::<MIN_BUDDY_SIZE>(MIN_BUDDY_SIZE * 8 - 4),
            ))
            .unwrap();
        }
    }
    mod constructor {
        use super::*;
        #[test]
        fn minimal_mem_block() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[..MIN_BUDDY_SIZE * 2]
            });
        }
        #[should_panic]
        #[test]
        fn too_small_mem_block() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[..MIN_BUDDY_SIZE]
            });
        }
        #[test]
        fn maximal_mem_block() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                std::slice::from_raw_parts_mut(MEMORY_FIELD.array.as_mut_ptr(), MAX_BUDDY_SIZE)
            });
        }
        #[should_panic]
        #[test]
        fn too_big_mem_block() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                std::slice::from_raw_parts_mut(
                    MEMORY_FIELD.array.as_mut_ptr(),
                    MAX_BUDDY_SIZE + 0x1000,
                )
            });
        }
        #[test]
        fn aligned_mem_block1() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[MIN_BUDDY_SIZE * 2..MIN_BUDDY_SIZE * 4]
            });
        }
        #[should_panic]
        #[test]
        fn bad_aligned_mem_block1() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[4..MIN_BUDDY_SIZE * 2 + 4]
            });
        }
        #[test]
        fn aligned_mem_block2() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[MIN_BUDDY_SIZE * 8..MIN_BUDDY_SIZE * 16]
            });
        }
        #[should_panic]
        #[test]
        fn bad_aligned_mem_block2() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[MIN_BUDDY_SIZE * 9..MIN_BUDDY_SIZE * 17]
            });
        }
        #[test]
        fn aligned_mem_block3() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[MAX_SUPPORTED_ALIGN..MAX_SUPPORTED_ALIGN * 17]
            });
        }
        #[should_panic]
        #[test]
        fn bad_aligned_mem_block3() {
            ProtectedAllocator::<MIN_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[(MAX_SUPPORTED_ALIGN / 2)
                    ..(MAX_SUPPORTED_ALIGN * 16) + (MAX_SUPPORTED_ALIGN / 2)]
            });
        }
        #[test]
        fn generic_size_changed() {
            ProtectedAllocator::<{ MIN_BUDDY_SIZE * 2 }>::new(unsafe {
                &mut MEMORY_FIELD.array[..MIN_BUDDY_SIZE * 4]
            });
        }
        #[should_panic]
        #[test]
        fn generic_below_min_size() {
            ProtectedAllocator::<{ MIN_BUDDY_SIZE / 2 }>::new(unsafe {
                &mut MEMORY_FIELD.array[..MIN_BUDDY_SIZE * 4]
            });
        }

        #[should_panic]
        #[test]
        fn generic_above_min_size() {
            ProtectedAllocator::<MAX_BUDDY_SIZE>::new(unsafe {
                &mut MEMORY_FIELD.array[..MAX_BUDDY_SIZE]
            });
        }
        #[should_panic]
        #[test]
        fn generic_unaligned_min_size() {
            ProtectedAllocator::<{ MIN_BUDDY_SIZE / 2 * 3 }>::new(unsafe {
                &mut MEMORY_FIELD.array[..MAX_BUDDY_SIZE]
            });
        }
    }
}
