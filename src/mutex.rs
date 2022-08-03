//! Generic mutex traits
//! Hugely copied from Trait Mutex crate
//!
//! The traits in this module allow code to be generic over the mutex type used.
//! The types implementing these traits must guarantee that access is always
//! exclusive, even for a `RoMutex`.

use core::fmt::Debug;

/// Simple creation of a new Mutex
pub trait GenericMutex<T>: Sized {
    /// Creation errpr
    type CreationError: Debug;

    /// Create a new Mutex
    fn create(v: T) -> Result<Self, Self::CreationError>;
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

#[cfg(all(not(feature = "no-std"), not(feature = "no-generic-std-mutex-impl")))]
mod std_mutex {
    use super::{GenericMutex, RwMutex};

    use std::sync::Mutex;

    impl<T> const GenericMutex<T> for Mutex<T> {
        type CreationError = ();

        #[inline(always)]
        fn create(v: T) -> Result<Self, Self::CreationError> {
            Ok(Mutex::new(v))
        }
    }

    impl<T> RwMutex<T> for Mutex<T> {
        type Error = ();

        #[inline(always)]
        fn lock_mut<R>(&self, f: impl FnOnce(&mut T) -> R) -> Result<R, Self::Error> {
            let mut v = self.lock().unwrap();
            Ok(f(&mut v))
        }
    }
}
