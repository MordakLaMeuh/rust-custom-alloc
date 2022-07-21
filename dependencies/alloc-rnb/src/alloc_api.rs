use super::SimpleAllocator;

use std::alloc::GlobalAlloc;
use std::alloc::{handle_alloc_error, AllocError, Allocator, Layout};
use std::ptr::NonNull;

unsafe impl Allocator for SimpleAllocator {
    fn allocate(&self, layout: Layout) -> Result<NonNull<[u8]>, AllocError> {
        let ptr = unsafe { self.alloc(layout) };
        if ptr.is_null() {
            handle_alloc_error(layout);
        }
        unsafe {
            Ok(NonNull::new_unchecked(std::slice::from_raw_parts_mut(
                ptr,
                layout.size(),
            )))
        }
    }
    unsafe fn deallocate(&self, ptr: NonNull<u8>, layout: Layout) {
        self.dealloc(ptr.as_ptr(), layout);
    }
}
