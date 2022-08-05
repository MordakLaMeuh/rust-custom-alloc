#[cfg(target_pointer_width = "32")]
mod math32;
#[cfg(target_pointer_width = "32")]
pub use math32::{round_up_2, trailing_zero_right};
#[cfg(target_pointer_width = "64")]
mod math64;
#[cfg(target_pointer_width = "64")]
pub use math64::{round_up_2, trailing_zero_right};
