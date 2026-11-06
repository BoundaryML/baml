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
/// Loop-owned synchronous root access for runtime integration. The task context
/// is deliberately not a RootSource. Root ranges cannot escape the callback.
pub struct IntegrationRoots(Rc<RefCell<Core>>);
/// A precise pointer slot registered with the owning loop. Dropping the lease
/// releases the root without borrowing the loop (including during fiber unwind).
/// The lease never provides access to the loop's root enumeration.
pub struct IntegrationRoot(Rc<Cell<usize>>);
impl IntegrationRoot {
    pub fn get(&self) -> usize {
        self.0.get()
    }
}
impl IntegrationRoots {
    /// Shared owner-thread checkpoint word. Bit 0 is maintained by the loop;
    /// bit 1 is reserved for the integrated heap. Callers must preserve bit 0.
    pub fn poll_word(&self) -> Rc<Cell<u8>> {
        self.0.borrow().poll_word.clone()
    }

    /// Register a managed pointer held outside the conservatively scanned stacks.
    ///
    /// # Safety
    /// `pointer` must be null or a live pointer in this loop's heap. The pointee
    /// may be accessed only while both this lease and that heap remain alive.
    pub unsafe fn root(&self, pointer: usize) -> IntegrationRoot {
        let slot = Rc::new(Cell::new(pointer));
        let mut core = self.0.borrow_mut();
        core.integration_slots
            .retain(|slot| slot.strong_count() != 0);
        core.integration_slots.push(Rc::downgrade(&slot));
        IntegrationRoot(slot)
    }

    /// Stack bound for a task owned by this loop, for generated-code guards.
    pub fn stack_limit(&self, task: usize) -> usize {
        self.0.borrow().tasks[&task].stack_limit
    }

    pub fn with_roots<R>(&self, visit: impl FnOnce(&dyn RootSource) -> R) -> R {
        visit(&*self.0.borrow())
    }
}
impl TaskLoop {
    pub fn integration_roots(&self) -> IntegrationRoots {
        IntegrationRoots(self.core.clone())
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
        for slot in &self.integration_slots {
            if let Some(slot) = slot.upgrade() {
                visit(slot.as_ptr());
            }
        }
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
