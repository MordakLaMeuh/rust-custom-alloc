//! Generic mutex traits
//!
//! The traits in this module allow code to be generic over the mutex type used.
//! The types implementing these traits must guarantee that access is always
//! exclusive, even for a `RoMutex`.

use core::fmt::Debug;

/// Simple creation of a new Mutex
pub trait GenericMutex<T>: Sized {
    /// Create a new Mutex
    fn create(v: T) -> Self;
}

/// A read-only (immutable) mutex.
///
/// This means, the value it shares is immutable, but only a single context may
/// have exclusive access.
pub trait RoMutex<T>: GenericMutex<T> {
    /// Locking error
    type Error: Debug;

    /// Lock the mutex for the duration of a closure
    ///
    /// `lock` will call a closure with an immutable reference to the unlocked
    /// mutex's value.
    fn lock<R>(&self, f: impl FnOnce(&T) -> R) -> Result<R, Self::Error>;
}

/// A read-write (mutable) mutex.
///
/// This mutex type is similar to the Mutex from `std`.  When you lock it, you
/// get access to a mutable reference.
pub trait RwMutex<T>: GenericMutex<T> {
    /// Locking error
    type Error: Debug;

    /// Lock the mutex for the duration of a closure
    ///
    /// `lock_mut` will call a closure with a mutable reference to the unlocked
    /// mutex's value.
    fn lock_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, Self::Error>;
}

#[cfg(not(feature = "no-std"))]
mod std {
    use super::{GenericMutex, RwMutex};
    use std::sync::Mutex;

    impl<T> const GenericMutex<T> for Mutex<T> {
        fn create(v: T) -> Self {
            Mutex::new(v)
        }
    }

    impl<T> RwMutex<T> for Mutex<T> {
        type Error = ();

        fn lock_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, Self::Error> {
            let mut v = self.lock().unwrap();
            Ok(f(&mut v))
        }
    }
}
