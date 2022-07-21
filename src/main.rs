#![cfg_attr(feature = "allocator_trait", feature(allocator_api))]

use alloc_rnb::SimpleAllocator;
use alloc_rnb::ARENA_SIZE;

use std::sync::atomic::Ordering::Acquire;

#[global_allocator]
static ALLOCATOR: SimpleAllocator = SimpleAllocator::init();

fn main() {
    let _s = format!("allocating a string!");

    #[cfg(feature = "allocator_trait")]
    {
        let b = Box::new_in(42, &ALLOCATOR);
        dbg!(b);
    }
    let currently = ALLOCATOR.remaining.load(Acquire);
    println!("allocated so far: {}", ARENA_SIZE - currently);
}
