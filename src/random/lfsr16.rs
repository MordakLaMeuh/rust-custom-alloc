//! see https://en.wikipedia.org/wiki/Linear-feedback_shift_register
const SEQ_SIZE: usize = 1 << 11;

struct LfsrFibonnaci {
    pub registers: [u32; SEQ_SIZE],
    pub current_offset: usize,
    pub stored_seed: Option<u16>,
}

/// Main structure
static mut LFSR_FIBONACCI: LfsrFibonnaci = LfsrFibonnaci {
    registers: [0; SEQ_SIZE],
    current_offset: 0,
    stored_seed: None,
};

/// Fibonacci LFSR
pub fn lfsr16_set_seed(seed: u16) {
    if seed == 0 {
        panic!("Seed must be greeter than o");
    } else {
        let mut lfsr: u16 = seed;
        unsafe {
            // lfsr fly time must be at 1 ^ 16
            // enumerator is only used for assert! check
            for (i, elem) in LFSR_FIBONACCI.registers.iter_mut().enumerate() {
                for j in 0..32 {
                    let bits: u16 = (lfsr >> 0) ^ (lfsr >> 2) ^ (lfsr >> 3) ^ (lfsr >> 5);
                    lfsr = lfsr >> 1;
                    // lfsr.set_bit(15, bits.get_bit(0));
                    // (*elem).set_bit(j, bits.get_bit(0));
                    let bit = bits & 0b1;
                    lfsr |= bit << 15;
                    *elem |= ((bit as u32) << j) as u32;

                    // check of algorythm mathematical coherency
                    assert!(
                        lfsr != seed || (lfsr == seed && i as usize == SEQ_SIZE - 1 && j == 30)
                    );
                }
            }
            LFSR_FIBONACCI.stored_seed = Some(seed);
        }
        // partial check of algorythm calculation success
        assert!(lfsr << 1 == seed & 0xfffe);
    }
}

/// move offset into flsr
#[inline(always)]
fn move_offset(offset: usize) -> usize {
    if offset == SEQ_SIZE - 1 {
        0
    } else {
        offset + 1
    }
}

/// get a pseudo random number from the lfsr fibonacci suite
pub fn lfsr16_get_pseudo_number() -> u32 {
    match unsafe { LFSR_FIBONACCI.stored_seed } {
        Some(_) => {
            let result: u32;
            unsafe {
                result = LFSR_FIBONACCI.registers[LFSR_FIBONACCI.current_offset];
                LFSR_FIBONACCI.current_offset = move_offset(LFSR_FIBONACCI.current_offset);
            }
            result
        }
        None => panic!("A seed must be set"),
    }
}
