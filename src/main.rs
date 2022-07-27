#![cfg_attr(feature = "allocator_trait", feature(allocator_api))]
#![feature(unchecked_math)]

use alloc_rnb::SimpleAllocator;
use alloc_rnb::ARENA_SIZE;

use std::sync::atomic::Ordering::Acquire;

struct Test<K, V> {
    a: K,
    b: V,
}

const BUDDY: &'static [u8] = &[0_u8; 1024 * 1024];

#[global_allocator]
static ALLOCATOR: SimpleAllocator = SimpleAllocator::init();

// Round up to the next highest power of 2
#[inline(always)]
fn round_up_2(mut v: u32) -> u32 {
    debug_assert_ne!(v, 0);
    v -= 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v += 1;
    v
}

const IDX_ARRAY: [u32; 32] = [
    0, 1, 28, 2, 29, 14, 24, 3, 30, 22, 20, 15, 25, 17, 4, 8, 31, 27, 13, 23, 21, 19, 16, 7, 26,
    12, 18, 6, 11, 5, 10, 9,
];

// Count the consecutive zero bits (trailing) on the right with multiply and lookup
#[inline(always)]
fn trailing_zero_right(v: u32) -> u32 {
    debug_assert_ne!(v, 0);
    debug_assert_eq!(
        -1_i32,
        i32::from_ne_bytes([0xff, 0xff, 0xff, 0xff]),
        "this machine doesnt handle negatives numbers with two's complement representation"
    );
    // C  => idx = bits_right[((uint32_t)((v & -v) * 0x077CB531U)) >> 27] with v @int
    // (v & (!v + 1)) is eq to (v & -v) in two's complement representation
    // unchecked_mul on Rust must output the same result like a lang C multiplication
    let idx = (unsafe { (v & (!v + 1)).unchecked_mul(0x077C_B531) } >> 27) as usize;
    IDX_ARRAY[idx]
}

fn main() {
    let _s = format!("allocating a string!");

    #[cfg(feature = "allocator_trait")]
    {
        let b = Box::new_in(42, &ALLOCATOR);
        dbg!(b);
    }
    let currently = ALLOCATOR.remaining.load(Acquire);
    println!("allocated so far: {}", ARENA_SIZE - currently);
    println!("{}", std::mem::size_of::<Test<u64, ()>>());
}

#[cfg(test)]
mod test {
    #[test]
    fn round_up_2() {
        fn dummy_round_up(v: u32) -> u32 {
            let mut power: u32 = 1;
            while power < v {
                power *= 2;
            }
            power
        }
        use super::round_up_2;
        // Test with somes numbers
        for i in FIBO.into_iter().filter(|i| **i != 0) {
            assert_eq!(round_up_2(*i), dummy_round_up(*i));
        }
        // Test for bundary
        for i in (0..32_u32).map(|i| 1 << i) {
            assert_eq!(round_up_2(i), dummy_round_up(i));
        }
    }
    #[test]
    fn trailing_zero_right() {
        fn dummy_trailing_zero_right(v: u32) -> u32 {
            let mut shr: u32 = 0;
            while shr < 32 {
                if (v >> shr) & 0b1 == 0b1 {
                    break;
                }
                shr += 1;
            }
            shr
        }
        use super::trailing_zero_right;
        // Test with somes numbers
        for i in FIBO.into_iter().filter(|i| **i != 0) {
            assert_eq!(trailing_zero_right(*i), dummy_trailing_zero_right(*i));
        }
        // Test for bundary
        for i in (0..32_u32).map(|i| 1 << i) {
            assert_eq!(trailing_zero_right(i), dummy_trailing_zero_right(i));
        }
    }
    const FIBO: &'static [u32] = &[
        0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 377, 610, 987, 1597, 2584, 4181, 6765,
        10946, 17711, 28657, 46368, 75025, 121393, 196418, 317811, 514229, 832040, 1346269,
        2178309, 3524578, 5702887, 9227465, 14930352, 24157817, 39088169, 63245986, 102334155,
        165580141, 267914296, 433494437, 701408733, 1134903170, 1836311903,
    ];
}
