//! Rust futures over the core: a [`Call`] per call, driven by whoever
//! polls.
//!
//! Built on the core's public functions alone ([`Library::load`],
//! [`Library::session`], [`Session::start`], [`Session::turn`],
//! [`BamlValue::detach`], [`Session::attach`]): what a Python or Node module
//! would use in place of this layer.
//!
//! Who turns: a `Call` that is polled gives the library a turn when one is
//! owed (a call started, the library's `wake` came, its timer fired), on
//! the polling thread. When a turn becomes owed, one waiting call is woken.
//! If that call is dropped, or ends, while the turn is still owed, it wakes
//! another in its place. Nothing of the user's runs while the library is
//! held: wakers are woken, and the hook for unhandled failures called, once
//! the session is dropped.

use std::{
    collections::{HashMap, HashSet},
    future::Future,
    marker::PhantomData,
    panic::AssertUnwindSafe,
    pin::Pin,
    sync::{
        Arc, Condvar, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll, Waker},
    time::Instant,
};

use crate::{BamlValue, Detached, Event, FromBaml, Library, Outcome, SdkError, Session, tag};

/// How a call failed. `E` is the function's declared `throws` type.
#[derive(Clone, Debug, PartialEq)]
pub enum Error<E = std::convert::Infallible> {
    /// It threw a value of its declared `throws` type.
    Thrown {
        value: Box<E>,
    },
    /// It threw something else: the class's name when it is an instance,
    /// and the value as text.
    Runtime {
        class_name: Option<String>,
        message: String,
    },
    /// It panicked (`baml.panics`, `baml.panics.Exit` included).
    Panic {
        message: String,
    },
    /// It can never finish: nothing can run, no timer is set, and no I/O
    /// can complete.
    Stuck,
    Sdk(SdkError),
}

impl<E: std::fmt::Debug> std::fmt::Display for Error<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Thrown { value } => write!(f, "BAML threw {value:?}"),
            Self::Runtime { message, .. } => write!(f, "BAML threw {message}"),
            Self::Panic { message } => write!(f, "BAML panicked: {message}"),
            Self::Stuck => write!(f, "the BAML call can never finish"),
            Self::Sdk(error) => write!(f, "{error}"),
        }
    }
}

impl<E: std::fmt::Debug> std::error::Error for Error<E> {}

impl<E> From<SdkError> for Error<E> {
    fn from(error: SdkError) -> Self {
        Self::Sdk(error)
    }
}

type Hook = Arc<dyn Fn(&str) + Send + Sync>;

/// The calls' side of a turn.
#[derive(Default)]
struct State {
    /// Calls that ended, not yet taken by their `Call`.
    ended: HashMap<u64, (Outcome, Detached)>,
    /// Calls that wait, with whoever to wake.
    waiting: HashMap<u64, Waker>,
    /// Calls whose `Call` was dropped: their results are dropped too.
    abandoned: HashSet<u64>,
}

/// When the library's next timer fires.
#[derive(Default)]
struct Timer {
    deadline: Mutex<Option<Instant>>,
    changed: Condvar,
}

struct Driver {
    library: &'static Library,
    state: Mutex<State>,
    /// A turn is owed: a call started, `wake` came or a timer fired.
    owed: AtomicBool,
    timer: Arc<Timer>,
    hook: RwLock<Hook>,
}

static DRIVER: OnceLock<Driver> = OnceLock::new();

/// What is done with the error of a task nobody awaited, unless the user
/// says otherwise.
#[expect(clippy::print_stderr, reason = "the default the user can replace")]
fn print_unhandled(error: &str) {
    eprintln!("baml: a task failed and nothing awaited it: {error}");
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Load the library at `path` for calls as Rust futures: [`Library::load`],
/// with this layer's `wake`, and its timer thread (one per process).
pub fn load(path: &str, expected_hash: u64) -> Result<&'static Library, SdkError> {
    let library = Library::load(path, expected_hash, || {
        if let Some(driver) = DRIVER.get() {
            driver.owe_turn();
        }
    })?;
    let mut started = false;
    DRIVER.get_or_init(|| {
        started = true;
        Driver {
            library,
            state: Mutex::new(State::default()),
            owed: AtomicBool::new(false),
            timer: Arc::new(Timer::default()),
            hook: RwLock::new(Arc::new(print_unhandled)),
        }
    });
    if started {
        start_timer();
    }
    Ok(library)
}

/// Set what is done with the error of a task nobody awaited: it is given
/// the error as text, with nothing of the library held, so it may call
/// anything. A panic in it is caught. The default prints it to stderr.
pub fn set_on_unhandled(hook: impl Fn(&str) + Send + Sync + 'static) {
    if let Some(driver) = DRIVER.get() {
        *driver
            .hook
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Arc::new(hook);
    }
}

/// The timer thread: when the library's next timer fires, a turn is owed.
fn start_timer() {
    std::thread::Builder::new()
        .name("baml-host-timer".to_owned())
        .spawn(|| {
            let Some(driver) = DRIVER.get() else { return };
            let timer = &driver.timer;
            let mut deadline = lock(&timer.deadline);
            loop {
                match *deadline {
                    None => {
                        deadline = timer
                            .changed
                            .wait(deadline)
                            .unwrap_or_else(std::sync::PoisonError::into_inner);
                    }
                    Some(when) => {
                        let now = Instant::now();
                        if now >= when {
                            *deadline = None;
                            drop(deadline);
                            driver.owe_turn();
                            deadline = lock(&timer.deadline);
                        } else {
                            deadline = timer
                                .changed
                                .wait_timeout(deadline, when - now)
                                .unwrap_or_else(std::sync::PoisonError::into_inner)
                                .0;
                        }
                    }
                }
            }
        })
        .expect("the timer thread starts");
}

impl Driver {
    /// A turn is owed: wake one waiting call to give it.
    fn owe_turn(&self) {
        self.owed.store(true, Ordering::Release);
        self.wake_one();
    }

    /// Wake one waiting call, if any waits.
    fn wake_one(&self) {
        let waker = lock(&self.state).waiting.values().next().cloned();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    /// Hand the turn over if it is still owed: the call that was to give it
    /// is ending or dropped.
    fn hand_over(&self) {
        if self.owed.load(Ordering::Acquire) {
            self.wake_one();
        }
    }

    /// Give the library a turn, keep what ended for the calls, then, with
    /// nothing held, wake the calls that ended and report failures nobody
    /// awaited.
    fn turn(&self) {
        let mut woken = Vec::new();
        let mut unhandled = Vec::new();
        let next = {
            let session = self.library.session();
            session.turn(|event| match event {
                Event::Ended {
                    call,
                    outcome,
                    value,
                } => {
                    let mut state = lock(&self.state);
                    if state.abandoned.remove(&call) {
                        drop(state);
                        drop(value);
                    } else {
                        state.ended.insert(call, (outcome, value.detach()));
                        if let Some(waker) = state.waiting.remove(&call) {
                            woken.push(waker);
                        }
                    }
                }
                Event::Unhandled { error, .. } => unhandled.push(error.describe()),
            })
        };
        if let Some(next) = next {
            *lock(&self.timer.deadline) = Some(Instant::now() + next);
            self.timer.changed.notify_one();
        }
        for waker in woken {
            waker.wake();
        }
        if !unhandled.is_empty() {
            let hook = Arc::clone(
                &self
                    .hook
                    .read()
                    .unwrap_or_else(std::sync::PoisonError::into_inner),
            );
            for error in unhandled {
                let _ = std::panic::catch_unwind(AssertUnwindSafe(|| hook(&error)));
            }
        }
    }
}

/// Start export `export` of `library` with the arguments `args` builds,
/// and return the future of its result. `library` was loaded with
/// [`load`].
pub fn call<T: FromBaml, E: FromBaml>(
    library: &'static Library,
    export: u32,
    args: impl FnOnce(&Session) -> Result<Vec<BamlValue<'_>>, SdkError>,
) -> Call<T, E> {
    let Some(driver) = DRIVER.get() else {
        return Call::failed(SdkError::Load(
            "the library was not loaded with `futures::load`".to_owned(),
        ));
    };
    let started = {
        let session = library.session();
        args(&session).and_then(|args| session.start(export, &args))
    };
    match started {
        Ok(id) => {
            driver.owed.store(true, Ordering::Release);
            Call {
                driver: Some(driver),
                id,
                failed: None,
                done: false,
                result: PhantomData,
            }
        }
        Err(error) => Call::failed(error),
    }
}

/// A call in flight: the future of its result. Dropping it does not cancel
/// the BAML call; its result is freed when it arrives.
pub struct Call<T, E> {
    driver: Option<&'static Driver>,
    id: u64,
    failed: Option<SdkError>,
    done: bool,
    result: PhantomData<fn() -> (T, E)>,
}

impl<T, E> Call<T, E> {
    fn failed(error: SdkError) -> Self {
        Self {
            driver: None,
            id: 0,
            failed: Some(error),
            done: false,
            result: PhantomData,
        }
    }
}

impl<T: FromBaml, E: FromBaml> Call<T, E> {
    /// Copy what the call ended with into Rust values.
    fn finish(driver: &Driver, outcome: Outcome, value: Detached) -> Result<T, Error<E>> {
        let session = driver.library.session();
        let value = session.attach(value);
        match outcome {
            Outcome::Returned => T::from_baml(value).map_err(Error::Sdk),
            Outcome::Threw => {
                let message = value.describe();
                let class_name = (value.kind() == tag::INSTANCE).then(|| {
                    message
                        .split([' ', '{'])
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                });
                match E::from_baml(value) {
                    Ok(thrown) => Err(Error::Thrown {
                        value: Box::new(thrown),
                    }),
                    Err(_) => Err(Error::Runtime {
                        class_name,
                        message,
                    }),
                }
            }
            Outcome::Stuck => Err(Error::Stuck),
            Outcome::Panicked => Err(Error::Panic {
                message: value.describe(),
            }),
        }
    }
}

impl<T: FromBaml, E: FromBaml> Future for Call<T, E> {
    type Output = Result<T, Error<E>>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Some(error) = self.failed.take() {
            self.done = true;
            return Poll::Ready(Err(Error::Sdk(error)));
        }
        let Some(driver) = self.driver else {
            return Poll::Ready(Err(Error::Sdk(SdkError::Load(
                "a call polled again after it ended".to_owned(),
            ))));
        };
        loop {
            let ended = {
                let mut state = lock(&driver.state);
                let ended = state.ended.remove(&self.id);
                if ended.is_none() {
                    // Registered before a turn is tried, so no wake is lost.
                    state.waiting.insert(self.id, context.waker().clone());
                }
                ended
            };
            if let Some((outcome, value)) = ended {
                self.done = true;
                let result = Self::finish(driver, outcome, value);
                driver.hand_over();
                return Poll::Ready(result);
            }
            if driver.owed.swap(false, Ordering::AcqRel) {
                driver.turn();
                continue;
            }
            return Poll::Pending;
        }
    }
}

impl<T, E> Drop for Call<T, E> {
    fn drop(&mut self) {
        let Some(driver) = self.driver else { return };
        if self.done {
            return;
        }
        let ended = {
            let mut state = lock(&driver.state);
            state.waiting.remove(&self.id);
            let ended = state.ended.remove(&self.id);
            if ended.is_none() {
                state.abandoned.insert(self.id);
            }
            ended
        };
        // Its result, if it came, is freed at the next session.
        drop(ended);
        driver.hand_over();
    }
}
