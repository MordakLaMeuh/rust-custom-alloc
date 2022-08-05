/// Round up to the next highest power of 2
#[inline(always)]
pub const fn round_up_2(mut v: usize) -> usize {
    debug_assert!(v != 0);
    v -= 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v += 1;
    v
}

const IDX_ARRAY: [usize; 32] = [
    0, 1, 28, 2, 29, 14, 24, 3, 30, 22, 20, 15, 25, 17, 4, 8, 31, 27, 13, 23, 21, 19, 16, 7, 26,
    12, 18, 6, 11, 5, 10, 9,
];

/// Count the consecutive zero bits (trailing) on the right with multiply and lookup
#[inline(always)]
pub const fn trailing_zero_right(v: usize) -> usize {
    debug_assert!(v != 0);
    debug_assert!(
        -1_isize == isize::from_ne_bytes([0xff, 0xff, 0xff, 0xff]),
        "this machine doesnt handle negatives numbers with two's complement representation"
    );
    // C  => idx = bits_right[((uint32_t)((v & -v) * 0x077CB531U)) >> 27] with v @int
    // (v & (!v + 1)) is eq to (v & -v) in two's complement representation
    // .overflowing_mul on Rust must output the same result like a lang C multiplication
    let idx = (v & (!v + 1)).overflowing_mul(0x077C_B531).0 >> 27;
    IDX_ARRAY[idx]
}

#[cfg(test)]
mod test_32b {
    #[test]
    fn round_up_2() {
        fn dummy_round_up(v: usize) -> usize {
            let mut power: usize = 1;
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
        for i in (0..32_usize).map(|i| 1 << i) {
            assert_eq!(round_up_2(i), dummy_round_up(i));
        }
    }
    #[test]
    fn trailing_zero_right() {
        fn dummy_trailing_zero_right(v: usize) -> usize {
            let mut shr: usize = 0;
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
        for i in (0..32_usize).map(|i| 1 << i) {
            assert_eq!(trailing_zero_right(i), dummy_trailing_zero_right(i));
        }
    }
    const FIBO: &'static [usize] = &[
        0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55, 89, 144, 233, 377, 610, 987, 1597, 2584, 4181, 6765,
        10946, 17711, 28657, 46368, 75025, 121393, 196418, 317811, 514229, 832040, 1346269,
        2178309, 3524578, 5702887, 9227465, 14930352, 24157817, 39088169, 63245986, 102334155,
        165580141, 267914296, 433494437, 701408733, 1134903170, 1836311903,
    ];
}
