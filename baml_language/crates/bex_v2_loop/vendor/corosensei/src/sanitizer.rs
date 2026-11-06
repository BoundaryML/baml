//! Runtime support for address sanitizer.

use crate::stack::StackPointer;

#[cfg(feature = "sanitizer")]
extern "C" {
    fn __sanitizer_start_switch_fiber(
        fake_stack_save: *mut *mut u8,
        bottom: *const u8,
        size: usize,
    );
    fn __sanitizer_finish_switch_fiber(
        fake_stack_save: *mut u8,
        bottom_old: *mut *const u8,
        size_old: *mut usize,
    );
    fn __asan_unpoison_memory_region(addr: *const u8, size: usize);
}

/// Information about stack bounds which needs to be communicated to the
/// sanitizer runtime when switching stacks.
///
/// This is an empty struct when sanitizer support is not enabled.
#[derive(Clone, Copy)]
pub struct SanitizerFiber {
    #[cfg(feature = "sanitizer")]
    pub(crate) bottom: *const u8,
    #[cfg(feature = "sanitizer")]
    pub(crate) size: usize,
}

unsafe impl Send for SanitizerFiber {}
unsafe impl Sync for SanitizerFiber {}

#[cfg(feature = "sanitizer")]
impl SanitizerFiber {
    #[inline]
    pub(crate) unsafe fn start_switch(&self) -> *mut u8 {
        let mut fake_stack = core::ptr::null_mut();
        __sanitizer_start_switch_fiber(&mut fake_stack, self.bottom, self.size);
        fake_stack
    }

    #[inline]
    pub(crate) unsafe fn finish_switch(fake_stack: *mut u8) -> Self {
        let mut bottom = core::ptr::null();
        let mut size = 0;
        __sanitizer_finish_switch_fiber(fake_stack, &mut bottom, &mut size);
        Self { bottom, size }
    }

    #[inline]
    pub(crate) unsafe fn from_parent_link(parent_link: *mut StackPointer) -> *mut Self {
        parent_link.byte_add(crate::arch::PARENT_LINK_OFFSET).cast()
    }

    #[inline]
    pub(crate) unsafe fn unpoison_stack(&self) {
        __asan_unpoison_memory_region(self.bottom, self.size);
    }
}

#[cfg(not(feature = "sanitizer"))]
impl SanitizerFiber {
    #[inline]
    pub(crate) unsafe fn start_switch(&self) -> *mut u8 {
        core::ptr::dangling_mut()
    }

    #[inline]
    pub(crate) unsafe fn finish_switch(_fake_stack: *mut u8) -> Self {
        Self {}
    }

    #[inline]
    pub(crate) unsafe fn from_parent_link(_parent_link: *mut StackPointer) -> *mut Self {
        core::ptr::dangling_mut()
    }

    #[inline]
    pub(crate) unsafe fn unpoison_stack(&self) {}
}

#[inline]
#[allow(unused_variables)]
pub(crate) unsafe fn unpoison_stack_range(base: StackPointer, limit: StackPointer) {
    #[cfg(feature = "sanitizer")]
    __asan_unpoison_memory_region(limit.get() as *const u8, base.get() - limit.get());
}
