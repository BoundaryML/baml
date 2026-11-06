use core::marker::PhantomData;

use crate::stack;
#[cfg(feature = "default-stack")]
use crate::stack::DefaultStack;
use crate::trap::CoroutineTrapHandler;
use crate::{Coroutine, CoroutineResult, Yielder};

/// Variant of [`Coroutine`] which allows the use of non-`'static` lifetimes.
///
/// Specifically this allows:
/// - A borrowed stack.
/// - Lifetimes in `Input`, `Yield` and `Return`,
/// - Borrowed values in the coroutine body closure.
///
/// To ensure safety, this requires that the coroutine only be accessed within
/// a scope, at the end of which it is dropped. Within the scope, a
/// [`ScopedCoroutineRef`] is provided, which mirrors the API of
/// `Coroutine`.
///
/// # Example
///
/// ```
/// use corosensei::{ScopedCoroutine, Yielder};
///
/// let mut counter = 0;
/// let coroutine = ScopedCoroutine::new(|yielder: &Yielder<i32, ()>, input| {
///     if input == 0 {
///         return;
///     }
///     counter += input;
///     loop {
///         let input = yielder.suspend(());
///         if input == 0 {
///             break;
///         }
///         counter += input;
///     }
/// });
/// coroutine.scope(|mut coroutine| {
///     coroutine.as_mut().resume(1);
///     coroutine.as_mut().resume(2);
///     coroutine.as_mut().resume(3);
///     coroutine.as_mut().resume(4);
///     coroutine.as_mut().resume(5);
/// });
/// assert_eq!(counter, 15);
/// ```
#[cfg(not(feature = "default-stack"))]
pub struct ScopedCoroutine<'a, Input, Yield, Return, Stack: stack::Stack> {
    inner: Coroutine<Input, Yield, Return, Stack>,
    // Ensure any borrows in the coroutine body outlive the ScopedCoroutine.
    marker: PhantomData<&'a mut ()>,
}

/// Variant of [`Coroutine`] which allows the use of non-`'static` lifetimes.
///
/// Specifically this allows:
/// - A borrowed stack.
/// - Lifetimes in `Input`, `Yield` and `Return`,
/// - Borrowed values in the coroutine body closure.
///
/// To ensure safety, this requires that the coroutine only be accessed within
/// a scope, at the end of which it is dropped. Within the scope, a
/// [`ScopedCoroutineRef`] is provided, which mirrors the API of
/// `Coroutine`.
///
/// # Example
///
/// ```
/// use corosensei::ScopedCoroutine;
///
/// let mut counter = 0;
/// let coroutine = ScopedCoroutine::new(|yielder, input| {
///     if input == 0 {
///         return;
///     }
///     counter += input;
///     loop {
///         let input = yielder.suspend(());
///         if input == 0 {
///             break;
///         }
///         counter += input;
///     }
/// });
/// coroutine.scope(|mut coroutine| {
///     coroutine.as_mut().resume(1);
///     coroutine.as_mut().resume(2);
///     coroutine.as_mut().resume(3);
///     coroutine.as_mut().resume(4);
///     coroutine.as_mut().resume(5);
/// });
/// assert_eq!(counter, 15);
/// ```
#[cfg(feature = "default-stack")]
pub struct ScopedCoroutine<'a, Input, Yield, Return, Stack: stack::Stack = DefaultStack> {
    inner: Coroutine<Input, Yield, Return, Stack>,
    // Ensure any borrows in the coroutine body outlive the ScopedCoroutine.
    marker: PhantomData<&'a mut ()>,
}

#[cfg(feature = "default-stack")]
impl<'a, Input, Yield, Return> ScopedCoroutine<'a, Input, Yield, Return, DefaultStack> {
    /// Creates a new coroutine which will execute `func` on the given stack.
    ///
    /// This function creates a coroutine which, when resumed, will execute
    /// `body` to completion. When desired the `func` can suspend itself via
    /// [`Yielder::suspend`].
    pub fn new<F: 'a>(func: F) -> Self
    where
        F: FnOnce(&Yielder<Input, Yield>, Input) -> Return,
    {
        Self::with_stack(Default::default(), func)
    }
}

impl<'a, Input, Yield, Return, Stack: stack::Stack>
    ScopedCoroutine<'a, Input, Yield, Return, Stack>
{
    /// Creates a new coroutine which will execute `func` on the given stack.
    ///
    /// This function creates a coroutine which, when resumed, will execute
    /// `body` to completion. When desired the `func` can suspend itself via
    /// [`Yielder::suspend`].
    pub fn with_stack<F: 'a>(stack: Stack, func: F) -> Self
    where
        F: FnOnce(&Yielder<Input, Yield>, Input) -> Return,
    {
        Self {
            inner: unsafe { Coroutine::with_stack_unchecked(stack, func) },
            marker: PhantomData,
        }
    }

    /// Extracts the stack from a coroutine that has finished executing.
    ///
    /// This allows the stack to be re-used for another coroutine.
    pub fn into_stack(self) -> Stack {
        self.inner.into_stack()
    }

    /// Creates a scope in which the coroutine can be executed.
    ///
    /// This ensure that any lifetimes used by the coroutine do not escape the
    /// scope and that the coroutine is dropped at the end of the scope.
    pub fn scope<F, T>(mut self, f: F) -> T
    where
        F: FnOnce(ScopedCoroutineRef<'_, Input, Yield, Return, Stack>) -> T,
    {
        let coroutine = ScopedCoroutineRef {
            inner: &mut self.inner,
        };
        f(coroutine)
    }
}

/// Reference to a coroutine within a scope created by
/// [`ScopedCoroutine::scope`].
///
/// Note that, unlike normal mutable references, Rust will not automatically
/// re-borrow this type so you may need to use the
/// [`ScopedCoroutineRef::as_mut`] method when invoking methods that take a
/// mutable reference.
pub struct ScopedCoroutineRef<'a, Input, Yield, Return, Stack: stack::Stack> {
    inner: &'a mut Coroutine<Input, Yield, Return, Stack>,
}

impl<Input, Yield, Return, Stack: stack::Stack>
    ScopedCoroutineRef<'_, Input, Yield, Return, Stack>
{
    /// Reborrows the `ScopedCoroutineRef`.
    ///
    /// This is useful when the reference needs to be used multiple times.
    ///
    /// This is necessary before Rust only supports automatic reborrowing for
    /// plain mutable references.
    pub fn as_mut(&mut self) -> ScopedCoroutineRef<'_, Input, Yield, Return, Stack> {
        ScopedCoroutineRef { inner: self.inner }
    }

    /// Resumes execution of this coroutine.
    ///
    /// This function will transfer execution to the coroutine and resume from
    /// where it last left off.
    ///
    /// If the coroutine calls [`Yielder::suspend`] then this function returns
    /// [`CoroutineResult::Yield`] with the value passed to `suspend`.
    ///
    /// If the coroutine returns then this function returns
    /// [`CoroutineResult::Return`] with the return value of the coroutine.
    ///
    /// # Panics
    ///
    /// Panics if the coroutine has already finished executing.
    ///
    /// If the coroutine itself panics during execution then the panic will be
    /// propagated to this caller.
    pub fn resume(&mut self, val: Input) -> CoroutineResult<Yield, Return> {
        self.inner.resume(val)
    }

    /// Returns whether this coroutine has been resumed at least once.
    pub fn started(&self) -> bool {
        self.inner.started()
    }

    /// Returns whether this coroutine has finished executing.
    ///
    /// A coroutine that has returned from its initial function can no longer
    /// be resumed.
    pub fn done(&self) -> bool {
        self.inner.done()
    }

    /// Forcibly marks the coroutine as having completed, even if it is
    /// currently suspended in the middle of a function.
    ///
    /// # Safety
    ///
    /// This is equivalent to a `longjmp` all the way back to the initial
    /// function of the coroutine, so the same rules apply.
    ///
    /// This can only be done safely if there are no objects currently on the
    /// coroutine's stack that need to execute `Drop` code.
    pub unsafe fn force_reset(&mut self) {
        self.inner.force_reset()
    }

    /// Unwinds the coroutine stack, dropping any live objects that are
    /// currently on the stack. This is automatically called when the coroutine
    /// is dropped.
    ///
    /// If the coroutine has already completed then this function is a no-op.
    ///
    /// If the coroutine is currently suspended on a `Yielder::suspend` call
    /// then unwinding it requires the `unwind` feature to be enabled and
    /// for the crate to be compiled with `-C panic=unwind`.
    ///
    /// # Panics
    ///
    /// This function panics if the coroutine could not be fully unwound. This
    /// can happen for one of two reasons:
    /// - The `ForcedUnwind` panic that is used internally was caught and not
    ///   rethrown.
    /// - This crate was compiled without the `unwind` feature and the
    ///   coroutine is currently suspended in the yielder (`started && !done`).
    pub fn force_unwind(&mut self) {
        self.inner.force_unwind()
    }

    /// Returns a [`CoroutineTrapHandler`] which can be used to handle traps that
    /// occur inside the coroutine. Examples of traps that can be handled are
    /// invalid memory accesses and stack overflows.
    ///
    /// The returned [`CoroutineTrapHandler`] can be used in a trap handler to
    /// force the trapping coroutine to return with a specific value, after
    /// which is it considered to have completed and can no longer be resumed.
    ///
    /// Needless to say, this is extremely unsafe and must be used with extreme
    /// care. See [`CoroutineTrapHandler::setup_trap_handler`] for the exact
    /// safety requirements.
    pub fn trap_handler(&self) -> CoroutineTrapHandler<Return> {
        self.inner.trap_handler()
    }
}
