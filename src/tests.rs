mod random;
#[cfg(not(feature = "no-std"))]
use random::{srand_init, Rand};

use super::protected_allocator::*;
use super::*;

#[cfg(not(feature = "no-std"))]
mod allocator {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn fill_and_empty() {
        #[repr(align(4096))]
        struct MemChunk([u8; 256]);
        let mut chunk = MemChunk([0; 256]);
        let alloc = BuddyAllocator::new(
            Arc::new(Mutex::new(AddressSpace::<64>(chunk.0.as_mut_slice()))),
            None,
        );

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
        struct MemChunk([u8; MIN_CELL_LEN * MIN_BUDDY_NB]);
        let mut chunk = MemChunk([0; MIN_CELL_LEN * MIN_BUDDY_NB]);
        let alloc = BuddyAllocator::new(
            Arc::new(Mutex::new(AddressSpace::<MIN_CELL_LEN>(
                chunk.0.as_mut_slice(),
            ))),
            None,
        );
        let mut v = Vec::new();
        for _i in 0..3 {
            let b = Box::try_new_in([0_u8; MIN_CELL_LEN], &alloc);
            if let Err(_) = &b {
                panic!("Should be done");
            }
            v.push(b);
        }
        let g = Box::try_new_in([0_u8; MIN_CELL_LEN], &alloc);
        if let Ok(_v) = &g {
            panic!("Should Fail");
        }
    }
    #[test]
    fn minimal_with_other_generic() {
        #[repr(align(4096))]
        struct MemChunk([u8; MIN_CELL_LEN * MIN_BUDDY_NB * 2]);
        let mut chunk = MemChunk([0; MIN_CELL_LEN * MIN_BUDDY_NB * 2]);
        let alloc = BuddyAllocator::new(
            Arc::new(Mutex::new(AddressSpace::<{ MIN_CELL_LEN * 2 }>(
                chunk.0.as_mut_slice(),
            ))),
            None,
        );
        let mut v = Vec::new();
        for _i in 0..3 {
            let b = Box::try_new_in([0xaa_u8; MIN_CELL_LEN * 2], &alloc);
            if let Err(_) = &b {
                panic!("Should be done");
            }
            v.push(b);
        }
        let g = Box::try_new_in([0xbb_u8; MIN_CELL_LEN * 2], &alloc);
        if let Ok(_v) = &g {
            panic!("Should Fail");
        }
    }
    // ___ These tests are the most important ___
    const NB_TESTS: usize = 4096;
    const MO: usize = 1024 * 1024;
    const CHUNK_SIZE: usize = MO * 16;
    #[repr(align(4096))]
    struct MemChunk([u8; CHUNK_SIZE]);
    struct Entry<'a, T: Allocator> {
        content: Vec<u8, &'a T>,
        data: u8,
    }
    const ALLOC_SIZE: &[usize] = &[64, 128, 256, 512, 1024, 2048, 4096];
    fn repeat_test<T>(alloc: &T)
    where
        T: Allocator,
    {
        let mut v = Vec::new();
        for _ in 0..NB_TESTS {
            match bool::srand(true) {
                true if v.len() > 200 => {
                    let entry: Entry<T> = v.remove(usize::srand(v.len() - 1));
                    for s in entry.content.iter() {
                        if *s != entry.data {
                            panic!("Corrupted Memory...");
                        }
                    }
                }
                _ => {
                    let size = ALLOC_SIZE[usize::srand(ALLOC_SIZE.len() - 1)];
                    let data = u8::srand(u8::MAX);
                    let mut content = Vec::new_in(alloc);
                    for _ in 0..size {
                        content.push(data);
                    }
                    v.push(Entry { content, data });
                }
            }
        }
        drop(v); // Flush all the alocator content
    }
    fn final_test<T>(alloc: &T)
    where
        T: Allocator,
    {
        let mut v = Vec::new_in(alloc);
        v.try_reserve(MO * 6).unwrap(); // Take the right buffy order 1 inside the allocator
        for _ in 0..(MO * 6) {
            v.push(42_u8);
        }
        let out = v.try_reserve(MO * 6); // The allocator cannot handle that
        if let Ok(_) = &out {
            panic!("This allocation is impossible");
        }
    }
    static mut CHUNK: MemChunk = MemChunk([0; CHUNK_SIZE]);
    #[test]
    fn memory_sodomizer() {
        srand_init(10);
        for _ in 0..4 {
            let alloc = BuddyAllocator::new(
                Arc::new(Mutex::new(AddressSpace::<64>(unsafe {
                    CHUNK.0.as_mut_slice()
                }))),
                Some(|e| {
                    dbg!(e);
                }),
            );
            repeat_test(&alloc);
            final_test(&alloc);
        }
    }
    #[test]
    fn memory_sodomizer_multithreaded() {
        srand_init(21);
        let mut memory = vec![0x21_u8; CHUNK_SIZE + MAX_SUPPORTED_ALIGN];
        let (_prefix, aligned_memory, _suffix) = unsafe { memory.align_to_mut::<MemChunk>() };
        // thread::spawn can only take static reference so force the compiler by
        // transmuting to cast reference as static. And ensure you manually that
        // the object will continue to live.
        let refer = &mut aligned_memory[0].0;
        let refer_static = unsafe { std::mem::transmute::<&mut [u8], &'static mut [u8]>(refer) };
        let alloc = BuddyAllocator::new(
            Arc::new(Mutex::new(AddressSpace::<64>(refer_static))),
            Some(|e| {
                dbg!(e);
            }),
        );
        let mut thread_list = Vec::new();
        for _ in 0..4 {
            let clone = alloc.clone();
            thread_list.push(std::thread::spawn(move || {
                repeat_test(&clone);
            }));
        }
        for thread in thread_list.into_iter() {
            drop(thread.join());
        }
        final_test(&alloc);
    }
    const MIN_CELL_LEN: usize = 64;
    static STATIC_ALLOC: StaticBuddyAllocator<
        Mutex<StaticAddressSpace<CHUNK_SIZE, MIN_CELL_LEN>>,
        CHUNK_SIZE,
        MIN_CELL_LEN,
    > = StaticBuddyAllocator::new(
        Mutex::new(StaticAddressSpace::new()),
        Some(|e| {
            dbg!(<BuddyError as Into<&str>>::into(e));
        }),
    );
    #[test]
    fn memory_sodomizer_multithreaded_with_static() {
        srand_init(42);
        let mut thread_list = Vec::new();
        for _ in 0..4 {
            thread_list.push(std::thread::spawn(move || {
                repeat_test(&STATIC_ALLOC);
            }));
        }
        for thread in thread_list.into_iter() {
            drop(thread.join());
        }
        final_test(&STATIC_ALLOC);
    }
}
mod buddy_convert {
    use super::*;
    #[test]
    fn normal() {
        [
            (MIN_CELL_LEN / 4, MIN_CELL_LEN / 4, MIN_CELL_LEN),
            (MIN_CELL_LEN, MIN_CELL_LEN, MIN_CELL_LEN),
            (MIN_CELL_LEN / 4, MIN_CELL_LEN, MIN_CELL_LEN),
            (0, MIN_CELL_LEN, MIN_CELL_LEN),
            (0, MIN_CELL_LEN * 2, MIN_CELL_LEN * 2),
            (1, 1, MIN_CELL_LEN),
            (1, MIN_CELL_LEN * 2, MIN_CELL_LEN * 2),
            (MIN_CELL_LEN * 2 - 2, MIN_CELL_LEN, MIN_CELL_LEN * 2),
            (MIN_CELL_LEN + 1, MIN_CELL_LEN, MIN_CELL_LEN * 2),
            (MIN_CELL_LEN * 8, MIN_CELL_LEN, MIN_CELL_LEN * 8),
            (MIN_CELL_LEN * 32 + 1, MIN_CELL_LEN, MIN_CELL_LEN * 64),
            (MIN_CELL_LEN * 257, MIN_CELL_LEN, MIN_CELL_LEN * 512),
            (usize::MAX / 4 + 1, MAX_SUPPORTED_ALIGN, usize::MAX / 4 + 1),
        ]
        .into_iter()
        .for_each(|(size, align, buddy_size)| {
            let layout = Layout::from_size_align(size, align)
                .expect(format!("size {} align {}", size, align).as_str());
            assert_eq!(
                BuddySize::<MIN_CELL_LEN>::try_from(layout).unwrap().0,
                BuddySize::<MIN_CELL_LEN>(buddy_size.try_into().unwrap()).0,
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
        BuddySize::<MIN_CELL_LEN>::try_from(
            Layout::from_size_align(usize::MAX, MAX_SUPPORTED_ALIGN * 2).unwrap(),
        )
        .unwrap();
    }
    #[should_panic]
    #[test]
    fn unsuported_size_request() {
        BuddySize::<MIN_CELL_LEN>::try_from(
            Layout::from_size_align(usize::MAX - 0x1000_0000, MAX_SUPPORTED_ALIGN).unwrap(),
        )
        .unwrap();
    }
}
mod order_convert {
    use super::*;
    #[test]
    fn normal() {
        [
            (MIN_CELL_LEN, MIN_CELL_LEN, 0),
            (MIN_CELL_LEN * 2, MIN_CELL_LEN * 4, 1),
            (MIN_CELL_LEN * 4, MIN_CELL_LEN * 16, 2),
            (MIN_CELL_LEN, MIN_CELL_LEN * 64, 6),
            (MIN_CELL_LEN * 2, MIN_CELL_LEN * 64, 5),
            (MIN_CELL_LEN * 64, MIN_CELL_LEN * 256, 2),
            (MIN_CELL_LEN * 128, MIN_CELL_LEN * 256, 1),
            (MIN_CELL_LEN * 256, MIN_CELL_LEN * 256, 0),
        ]
        .into_iter()
        .for_each(|(curr, max, order)| {
            assert_eq!(
                Order::try_from((
                    BuddySize::<MIN_CELL_LEN>(curr),
                    BuddySize::<MIN_CELL_LEN>(max)
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
            BuddySize::<MIN_CELL_LEN>(MIN_CELL_LEN * 8),
            BuddySize::<MIN_CELL_LEN>(MIN_CELL_LEN * 4),
        ))
        .unwrap();
    }
    #[should_panic]
    #[test]
    fn bad_buddy_size() {
        Order::try_from((
            BuddySize::<MIN_CELL_LEN>(MIN_CELL_LEN * 2),
            BuddySize::<MIN_CELL_LEN>(MIN_CELL_LEN * 8 - 4),
        ))
        .unwrap();
    }
}
mod constructor {
    use super::*;
    const MEMORY_FIELD_SIZE: usize = 0x4000_0000;
    #[repr(align(4096))]
    struct MemoryField {
        pub array: [u8; MEMORY_FIELD_SIZE],
    }
    static mut MEMORY_FIELD: MemoryField = MemoryField {
        array: [0; MEMORY_FIELD_SIZE],
    };
    #[test]
    fn minimal_mem_block() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            &mut MEMORY_FIELD.array[..MIN_CELL_LEN * MIN_BUDDY_NB]
        })
        .init();
    }
    #[should_panic]
    #[test]
    fn too_small_mem_block() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe { &mut MEMORY_FIELD.array[..MIN_CELL_LEN] })
            .init();
    }
    #[test]
    fn maximal_mem_block() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            std::slice::from_raw_parts_mut(MEMORY_FIELD.array.as_mut_ptr(), MEMORY_FIELD_SIZE)
        })
        .init();
    }
    #[should_panic]
    #[test]
    fn too_big_mem_block() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            std::slice::from_raw_parts_mut(
                MEMORY_FIELD.array.as_mut_ptr(),
                MEMORY_FIELD_SIZE + 0x1000,
            )
        })
        .init();
    }
    #[test]
    fn aligned_mem_block1() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            &mut MEMORY_FIELD.array[MIN_CELL_LEN * 20..MIN_CELL_LEN * (20 + MIN_BUDDY_NB)]
        })
        .init();
    }
    #[should_panic]
    #[test]
    fn bad_aligned_mem_block1() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            &mut MEMORY_FIELD.array[4..MIN_CELL_LEN * 2 + 4]
        })
        .init();
    }
    #[test]
    fn aligned_mem_block2() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            &mut MEMORY_FIELD.array[MIN_CELL_LEN * 8..MIN_CELL_LEN * 16]
        })
        .init();
    }
    #[should_panic]
    #[test]
    fn bad_aligned_mem_block2() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            &mut MEMORY_FIELD.array[MIN_CELL_LEN * 9..MIN_CELL_LEN * 17]
        })
        .init();
    }
    #[test]
    fn aligned_mem_block3() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            &mut MEMORY_FIELD.array[MAX_SUPPORTED_ALIGN..MAX_SUPPORTED_ALIGN * 17]
        })
        .init();
    }
    #[should_panic]
    #[test]
    fn bad_aligned_mem_block3() {
        ProtectedAllocator::<MIN_CELL_LEN>(unsafe {
            &mut MEMORY_FIELD.array
                [(MAX_SUPPORTED_ALIGN / 2)..(MAX_SUPPORTED_ALIGN * 16) + (MAX_SUPPORTED_ALIGN / 2)]
        })
        .init();
    }
    #[test]
    fn generic_size_changed() {
        ProtectedAllocator::<{ MIN_CELL_LEN * 2 }>(unsafe {
            &mut MEMORY_FIELD.array[..MIN_CELL_LEN * MIN_BUDDY_NB * 2]
        })
        .init();
    }
    #[should_panic]
    #[test]
    fn generic_below_min_size() {
        ProtectedAllocator::<{ MIN_CELL_LEN / 2 }>(unsafe {
            &mut MEMORY_FIELD.array[..MIN_CELL_LEN * MIN_BUDDY_NB]
        })
        .init();
    }

    #[should_panic]
    #[test]
    fn generic_above_min_size() {
        ProtectedAllocator::<MEMORY_FIELD_SIZE>(unsafe {
            &mut MEMORY_FIELD.array[..MEMORY_FIELD_SIZE]
        })
        .init();
    }
    #[should_panic]
    #[test]
    fn generic_unaligned_min_size() {
        ProtectedAllocator::<{ MIN_CELL_LEN / 2 * 3 }>(unsafe {
            &mut MEMORY_FIELD.array[..MEMORY_FIELD_SIZE]
        })
        .init();
    }
}
