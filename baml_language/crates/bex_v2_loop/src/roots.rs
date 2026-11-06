use super::*;

/// The section 2.3 seam, kept independent until the heap crate is integrated.
/// Visitors run synchronously; addresses must not escape the visit. Conservative
/// scanning is an ABI-specific unsafe operation, not portable Rust reflection.
pub trait RootSource {
    fn conservative_ranges(&self, visit: &mut dyn FnMut(*const usize, *const usize));
    fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize));
}
impl RootSource for TaskLoop {
    fn conservative_ranges(&self, visit: &mut dyn FnMut(*const usize, *const usize)) {
        self.core.borrow().conservative_ranges(visit);
    }
    fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize)) {
        self.core.borrow().precise_roots(visit);
    }
}
impl RootSource for Core {
    fn conservative_ranges(&self, visit: &mut dyn FnMut(*const usize, *const usize)) {
        // This non-inlined call spills callee-saved registers before visiting.
        // Caller-saved live values are spilled by the calling convention.
        with_registers(|sp, registers| {
            let base = self.running_base.unwrap_or_else(native_stack_base);
            visit(sp as *const usize, base as *const usize);
            visit(
                registers.as_ptr(),
                registers.as_ptr().wrapping_add(registers.len()),
            );
            for task in self.tasks.values() {
                if let Some(fiber) = &task.fiber
                    && let Some(sp) = fiber.saved_sp()
                {
                    visit(sp as *const usize, task.base as *const usize);
                }
            }
        });
    }
    fn precise_roots(&self, visit: &mut dyn FnMut(*mut usize)) {
        for task in self.tasks.values() {
            for value in &task.roots {
                value.roots(visit);
            }
        }
        for f in &self.futures {
            if let Some(f) = f.upgrade()
                && let FutureState::Ready(v) = &f.borrow().state
            {
                v.roots(visit);
            }
        }
        // Pending I/O completions are OwnedValue, and contain no heap pointers.
        // Hydrated completion values become future/task slots on this thread.
    }
}
#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "aarch64", target_arch = "x86_64")
)))]
compile_error!("v2 root spilling currently supports Linux aarch64/x86_64 only");
fn native_stack_base() -> usize {
    // SAFETY: pthread fills attr for the calling thread; destroy after reading.
    unsafe {
        let mut attr = std::mem::MaybeUninit::uninit();
        assert_eq!(
            libc::pthread_getattr_np(libc::pthread_self(), attr.as_mut_ptr()),
            0
        );
        let mut attr = attr.assume_init();
        let mut addr = std::ptr::null_mut();
        let mut size = 0;
        assert_eq!(libc::pthread_attr_getstack(&attr, &mut addr, &mut size), 0);
        libc::pthread_attr_destroy(&mut attr);
        addr as usize + size
    }
}
#[inline(never)]
fn with_registers(f: impl FnOnce(usize, &[usize])) {
    let mut regs = [0usize; 20];
    let sp: usize;
    // SAFETY: stores only within regs; observes the ABI's preserved registers.
    // No nostack/nomem options: compiler must keep the buffer addressable.
    #[cfg(target_arch = "aarch64")]
    unsafe {
        std::arch::asm!(
            "stp x19, x20, [{buf}, #0]", "stp x21, x22, [{buf}, #16]",
            "stp x23, x24, [{buf}, #32]", "stp x25, x26, [{buf}, #48]",
            "stp x27, x28, [{buf}, #64]", "stp x29, x30, [{buf}, #80]",
            "stp d8, d9, [{buf}, #96]", "stp d10, d11, [{buf}, #112]",
            "stp d12, d13, [{buf}, #128]", "stp d14, d15, [{buf}, #144]",
            "mov {sp}, sp", buf = in(reg) regs.as_mut_ptr(), sp = out(reg) sp,
        );
    }
    #[cfg(target_arch = "x86_64")]
    unsafe {
        std::arch::asm!(
            "mov [{buf}], rbx", "mov [{buf} + 8], rbp", "mov [{buf} + 16], r12",
            "mov [{buf} + 24], r13", "mov [{buf} + 32], r14", "mov [{buf} + 40], r15",
            "mov {sp}, rsp", buf = in(reg) regs.as_mut_ptr(), sp = out(reg) sp,
        );
    }
    f(sp, &regs);
    std::hint::black_box(regs);
}
