//! Generic mutex trait
//! Hugely copied from Trait Mutex crate
//!
//! The trait in this module allow code to be generic over the mutex type used.

use core::fmt::Debug;

/// A read-write (mutable) mutex trait.
///
/// When you lock it, you get access to a mutable reference.
/// The types implementing this trait must guarantee that access is always
/// exclusive. Must be implemented in no-std environment.
pub trait RwMutex<T>: Sized {
    /// Locking error
    type Error: Debug;

    /// Lock the mutex for the duration of a closure
    ///
    /// `lock_mut` will call a closure with a mutable reference to the unlocked
    /// mutex's value.
    fn lock_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, Self::Error>;
}

#[cfg(all(not(feature = "no-std"), not(feature = "no-generic-std-mutex-impl")))]
mod std_mutex {
    use super::RwMutex;

    use std::sync::Mutex;

    impl<T> RwMutex<T> for Mutex<T> {
        type Error = ();

        #[inline(always)]
        fn lock_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, Self::Error> {
            let mut v = self.lock().unwrap();
            Ok(f(&mut v))
        }
    }
}
