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
    v |= v >> 32;
    v += 1;
    v
}

const IDX_ARRAY: [usize; 64] = [
    0, 1, 2, 53, 3, 7, 54, 27, 4, 38, 41, 8, 34, 55, 48, 28, 62, 5, 39, 46, 44, 42, 22, 9, 24, 35,
    59, 56, 49, 18, 29, 11, 63, 52, 6, 26, 37, 40, 33, 47, 61, 45, 43, 21, 23, 58, 17, 10, 51, 25,
    36, 32, 60, 20, 57, 16, 50, 31, 19, 15, 30, 14, 13, 12,
];

/// Count the consecutive zero bits (trailing) on the right with multiply and lookup
#[inline(always)]
pub const fn trailing_zero_right(v: usize) -> usize {
    debug_assert!(v != 0);
    debug_assert!(
        -1_isize == isize::from_ne_bytes([0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]),
        "this machine doesnt handle negatives numbers with two's complement representation"
    );
    // C  => idx = bits_right[((uint64_t)((v & -v) * 0x22fdd63cc95386dULL)) >> 58] with v @long long int
    // (v & (!v + 1)) is eq to (v & -v) in two's complement representation
    // .overflowing_mul on Rust must output the same result like a lang C multiplication
    let idx = (v & (!v + 1)).overflowing_mul(0x22f_dd63_cc95_386d).0 >> 58;
    IDX_ARRAY[idx]
}

#[cfg(test)]
mod test_64b {
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
            while shr < 64 {
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
        for i in (0..64_usize).map(|i| 1 << i) {
            assert_eq!(trailing_zero_right(i), dummy_trailing_zero_right(i));
        }
    }
    const FIBO: &'static [usize] = &[
        0,
        1,
        1,
        2,
        3,
        5,
        8,
        13,
        21,
        34,
        55,
        89,
        144,
        233,
        377,
        610,
        987,
        1597,
        2584,
        4181,
        6765,
        10946,
        17711,
        28657,
        46368,
        75025,
        121393,
        196418,
        317811,
        514229,
        832040,
        1346269,
        2178309,
        3524578,
        5702887,
        9227465,
        14930352,
        24157817,
        39088169,
        63245986,
        102334155,
        165580141,
        267914296,
        433494437,
        701408733,
        1134903170,
        1836311903,
        2971215073,
        4807526976,
        7778742049,
        12586269025,
        20365011074,
        32951280099,
        53316291173,
        86267571272,
        139583862445,
        225851433717,
        365435296162,
        591286729879,
        956722026041,
        1548008755920,
        2504730781961,
        4052739537881,
        6557470319842,
        10610209857723,
        17167680177565,
        27777890035288,
        44945570212853,
        72723460248141,
        117669030460994,
        190392490709135,
        308061521170129,
        498454011879264,
        806515533049393,
        1304969544928657,
        2111485077978050,
        3416454622906707,
        5527939700884757,
        8944394323791464,
        14472334024676221,
        23416728348467685,
        37889062373143906,
        61305790721611591,
        99194853094755497,
        160500643816367088,
        259695496911122585,
        420196140727489673,
        679891637638612258,
        1100087778366101931,
        1779979416004714189,
        2880067194370816120,
        4660046610375530309,
        7540113804746346429,
    ];
}
