//! This module provides randomize functions

mod lfsr16;
use lfsr16::{lfsr16_get_pseudo_number, lfsr16_set_seed};

/// Has provide two methods
/// rand is totally undetermined and use RDRAND cpu feature (ivybridge +)
/// srand is seeded based random and use a seed algorythm
pub trait Rand {
    /// Rand based on a seed (must be initialized)
    fn srand(self) -> Self;
}

/// For now, lfsr16 is the only one method for srand, implentation may be extended in future
pub fn srand_init(seed: u16) {
    lfsr16_set_seed(seed)
}

/// f32 rand: -self..+self as f32
impl Rand for f32 {
    /// [i32::MIN..i32::MAX] € Z -> [+1..~-1] € D -> [+self..-self] € D
    fn srand(self) -> f32 {
        let t: i32 = lfsr16_get_pseudo_number() as i32;
        t as f32 / i32::MIN as f32 * self as f32
    }
}

/// i32 rand: -self..+self as i32
impl Rand for i32 {
    /// [i32::MIN..i32::MAX] € Z -> [+1..~-1] € D -> [+self..-self] € D -> [+self..-self] € Z
    fn srand(self) -> i32 {
        let t: i32 = lfsr16_get_pseudo_number() as i32;
        // lack of precision for i32 type with f32, usage of f32 instead
        (t as f32 / i32::MIN as f32 * self as f32).round() as i32
    }
}

/// isize rand: -self..+self as isize
impl Rand for isize {
    /// [isize::MIN..isize::MAX] € Z -> [+1..~-1] € D -> [+self..-self] € D -> [+self..-self] € Z
    fn srand(self) -> isize {
        let t: i32 = lfsr16_get_pseudo_number() as i32;
        // lack of precision for isize type with f32, usage of f32 instead
        (t as f32 / isize::MIN as f32 * self as f32).round() as isize
    }
}

/// i16 rand: -self..+self as i16
impl Rand for i16 {
    /// [i32::MIN..i32::MAX] € Z -> [+1..~-1] € D -> [+self..-self] € D -> [+self..-self] € Z
    fn srand(self) -> i16 {
        let t: i32 = lfsr16_get_pseudo_number() as i32;
        (t as f32 / i32::MIN as f32 * self as f32).round() as i16
    }
}

/// i8 rand: -self..+self as i8
impl Rand for i8 {
    /// [i32::MIN..i32::MAX] € Z -> [+1..~-1] € D -> [+self..-self] € D -> [+self..-self] € Z
    fn srand(self) -> i8 {
        let t: i32 = lfsr16_get_pseudo_number() as i32;
        (t as f32 / i32::MIN as f32 * self as f32).round() as i8
    }
}

/// u32 rand: 0..+self as u32
impl Rand for u32 {
    /// [0..u32::MAX] € N -> [0..+1] € D -> [0..+self] € D -> [0..+self] € N
    fn srand(self) -> u32 {
        let t: u32 = lfsr16_get_pseudo_number();
        // lack of precision for u32 type with f32, usage of f32 instead
        (t as f32 / u32::MAX as f32 * self as f32).round() as u32
    }
}

/// usize rand: 0..+self as usize
impl Rand for usize {
    /// [0..usize::MAX] € N -> [0..+1] € D -> [0..+self] € D -> [0..+self] € N
    fn srand(self) -> usize {
        let t: u32 = lfsr16_get_pseudo_number();
        // lack of precision for u32 type with f32, usage of f32 instead
        (t as f32 / usize::MAX as f32 * self as f32).round() as usize
    }
}

/// u16 rand: 0..+self as u16
impl Rand for u16 {
    /// [0..u32::MAX] € N -> [0..+1] € D -> [0..+self] € D -> [0..+self] € N
    fn srand(self) -> u16 {
        let t: u32 = lfsr16_get_pseudo_number();
        (t as f32 / u32::MAX as f32 * self as f32).round() as u16
    }
}

/// u8 rand: 0..+self as u8
impl Rand for u8 {
    /// [0..u32::MAX] € N -> [0..+1] € D -> [0..+self] € D -> [0..+self] € N
    fn srand(self) -> u8 {
        let t: u32 = lfsr16_get_pseudo_number();
        (t as f32 / u32::MAX as f32 * self as f32).round() as u8
    }
}

/// bool rand: 0..1 as bool
impl Rand for bool {
    /// [0..u32::MAX] € N -> &0b1 [FALSE | TRUE]
    fn srand(self) -> bool {
        let t: u32 = lfsr16_get_pseudo_number();
        match t & 0b1 {
            0 => false,
            1 => true,
            _ => panic!("woot ? Cannot happen"),
        }
    }
}

#[cfg(test)]
mod test {
    use super::{srand_init, Rand};

    #[test]
    fn random_out_of_bound_i16_test() {
        srand_init(42);
        for i in (i16::MIN..0).into_iter().step_by(128) {
            let x: i16 = i.srand();
            let limit_high = match i {
                i16::MIN => i16::MAX,
                _ => -1 * i,
            };
            assert!(x >= i && x <= limit_high);
        }
    }
    #[test]
    fn random_out_of_bound_i32_test() {
        srand_init(42);
        for i in (i32::MIN..0).into_iter().step_by(4096) {
            // test signed 32
            let x: i32 = i.srand();
            let limit_high = match i {
                i32::MIN => i32::MAX,
                _ => -1 * i,
            };
            assert!(x >= i && x <= limit_high);
        }
    }
    #[test]
    fn random_out_of_bound_u16_test() {
        srand_init(42);
        for i in (0..u16::MAX).into_iter().step_by(128) {
            // test unsigned 16
            let x: u16 = i.srand();
            assert!(x <= i);
        }
    }
    #[test]
    fn random_out_of_bound_u32_test() {
        srand_init(42);
        for i in (0..u32::MAX).into_iter().step_by(4096) {
            // test unsigned 32
            let x: u32 = i.srand();
            assert!(x <= i);
        }
    }
    #[test]
    fn random_out_of_bound_f32_test() {
        srand_init(42);
        for i in (0..u32::MAX).into_iter().step_by(4096) {
            // test f32
            let x: f32 = (i as f32).srand();
            assert!(x >= (i as f32 * -1.) && x <= i as f32);
        }
    }
}
