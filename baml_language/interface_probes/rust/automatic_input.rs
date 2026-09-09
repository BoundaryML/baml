//! Isolated proposed Rust SDK API for automatic `Arc<H: GreeterHost>` input.
//!
//! This models generated, per-interface code. It deliberately does not model a
//! universal "inspect any Rust trait object and discover all BAML interfaces"
//! operation. A combined host implementation supplies one explicit descriptor
//! bundle when it is registered.
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicU64, AtomicUsize, Ordering},
    },
    task::{Context, Poll, Wake, Waker},
};

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

fn ready<F: Future>(future: F) -> F::Output {
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    match std::pin::pin!(future).as_mut().poll(&mut context) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("this probe intentionally uses only ready futures"),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct DescriptorBundle {
    interfaces: &'static [&'static str],
}

static GREETER_ONLY: DescriptorBundle = DescriptorBundle {
    interfaces: &["probe.Greeter"],
};
static GREETER_AND_OTHER: DescriptorBundle = DescriptorBundle {
    interfaces: &["probe.Greeter", "probe.Other"],
};

#[derive(Clone, Copy)]
struct HostCallContext;

#[derive(Debug, PartialEq, Eq)]
enum CallError {
    MissingGreeterDescriptor,
    DescriptorBundleMismatch,
}

// This keeps the current generated host-trait shape: object-safe because its
// async operation is represented by a lifetime-generic boxed future.
trait GreeterHost: Send + Sync + 'static {
    fn greet<'a>(
        &'a self,
        name: String,
        ctx: HostCallContext,
    ) -> BoxFuture<'a, Result<String, CallError>>;

    // Explicit metadata, not generalized reflection over Rust trait impls. A
    // host implementing several generated BAML contracts overrides this once
    // with the generated/validated combined bundle.
    fn baml_descriptor_bundle(&self) -> &'static DescriptorBundle {
        &GREETER_ONLY
    }
}

struct HostSource {
    receiver_key: usize,
    host: Arc<dyn GreeterHost>,
    descriptors: &'static DescriptorBundle,
}

impl Clone for HostSource {
    fn clone(&self) -> Self {
        Self {
            receiver_key: self.receiver_key,
            host: self.host.clone(),
            descriptors: self.descriptors,
        }
    }
}

enum PreparedGreeterInput {
    CheckedView(GreeterRef),
    HostSource(HostSource),
}

impl Clone for PreparedGreeterInput {
    fn clone(&self) -> Self {
        match self {
            Self::CheckedView(view) => Self::CheckedView(view.clone()),
            Self::HostSource(source) => Self::HostSource(source.clone()),
        }
    }
}

// Generated per-interface input conversion. Preparation is local and
// synchronous: it acquires ownership but does not touch a BAML runtime.
trait GreeterInput {
    fn prepare_greeter_input(&self) -> PreparedGreeterInput;
}

struct Registration {
    registration_id: u64,
    receiver_key: usize,
    host: Arc<dyn GreeterHost>,
    descriptors: &'static DescriptorBundle,
}

struct Runtime {
    next_registration_id: AtomicU64,
    by_receiver: Mutex<HashMap<usize, Weak<Registration>>>,
}

impl Runtime {
    fn new() -> Self {
        Self {
            next_registration_id: AtomicU64::new(1),
            by_receiver: Mutex::new(HashMap::new()),
        }
    }

    async fn bind(&self, input: PreparedGreeterInput) -> Result<GreeterRef, CallError> {
        match input {
            PreparedGreeterInput::CheckedView(view) => Ok(view),
            PreparedGreeterInput::HostSource(source) => {
                if !source.descriptors.interfaces.contains(&"probe.Greeter") {
                    return Err(CallError::MissingGreeterDescriptor);
                }
                let mut registrations = self.by_receiver.lock().unwrap();
                if let Some(existing) = registrations
                    .get(&source.receiver_key)
                    .and_then(Weak::upgrade)
                {
                    // A second expected-interface path may not mutate or widen
                    // the witness set of an already-published host type.
                    if existing.descriptors != source.descriptors {
                        return Err(CallError::DescriptorBundleMismatch);
                    }
                    return Ok(GreeterRef(existing));
                }
                let registration = Arc::new(Registration {
                    registration_id: self.next_registration_id.fetch_add(1, Ordering::SeqCst),
                    receiver_key: source.receiver_key,
                    host: source.host,
                    descriptors: source.descriptors,
                });
                registrations.insert(source.receiver_key, Arc::downgrade(&registration));
                Ok(GreeterRef(registration))
            }
        }
    }

    async fn invoke_label_default(&self, input: PreparedGreeterInput) -> Result<String, CallError> {
        let _view = self.bind(input).await?;
        // Represents execution of `Greeter.label`'s BAML default body. It is
        // not copied into the extension trait or guessed from a Rust method.
        Ok("greeter".to_owned())
    }
}

struct CallContext {
    runtime: Arc<Runtime>,
}

struct GreeterRef(Arc<Registration>);

impl Clone for GreeterRef {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl GreeterRef {
    async fn greet(&self, name: String) -> Result<String, CallError> {
        self.0.host.greet(name, HostCallContext).await
    }

    fn registration_id(&self) -> u64 {
        self.0.registration_id
    }

    fn receiver_key(&self) -> usize {
        self.0.receiver_key
    }
}

impl GreeterInput for GreeterRef {
    fn prepare_greeter_input(&self) -> PreparedGreeterInput {
        PreparedGreeterInput::CheckedView(self.clone())
    }
}

// A generated concrete BAML facade has a direct checked view. It does not
// implement GreeterHost and therefore cannot overlap the Arc<H> blanket impl.
struct FriendlyGreeterFacade {
    greeter_view: GreeterRef,
}

impl GreeterInput for FriendlyGreeterFacade {
    fn prepare_greeter_input(&self) -> PreparedGreeterInput {
        PreparedGreeterInput::CheckedView(self.greeter_view.clone())
    }
}

// Proposed primary API. The generated interface namespace/factory fixes the
// complete Greeter + requires descriptor set synchronously and owns the host in
// an Arc. It never tries to enumerate other Rust traits implemented by H.
struct Greeter;

struct GreeterImplementation {
    source: HostSource,
}

impl Clone for GreeterImplementation {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
        }
    }
}

impl Greeter {
    fn implement<H: GreeterHost>(host: H) -> GreeterImplementation {
        let host: Arc<dyn GreeterHost> = Arc::new(host);
        GreeterImplementation {
            source: HostSource {
                receiver_key: Arc::as_ptr(&host) as *const () as usize,
                host,
                descriptors: &GREETER_ONLY,
            },
        }
    }
}

impl GreeterInput for GreeterImplementation {
    fn prepare_greeter_input(&self) -> PreparedGreeterInput {
        PreparedGreeterInput::HostSource(self.source.clone())
    }
}

fn concrete_host_source<H: GreeterHost>(value: &Arc<H>) -> HostSource {
    let host: Arc<dyn GreeterHost> = value.clone();
    HostSource {
        receiver_key: Arc::as_ptr(value) as *const () as usize,
        descriptors: host.baml_descriptor_bundle(),
        host,
    }
}

// `H` is implicitly Sized, leaving the unsized `dyn GreeterHost` case
// disjoint. Generated concrete facades/refs are different outer types, so
// these three input impl families do not overlap.
impl<H: GreeterHost> GreeterInput for Arc<H> {
    fn prepare_greeter_input(&self) -> PreparedGreeterInput {
        PreparedGreeterInput::HostSource(concrete_host_source(self))
    }
}

// Generic unsizing from Arc<H: ?Sized> to Arc<dyn GreeterHost> is not available
// for arbitrary H, so the already-erased case gets one explicit nonoverlapping
// implementation.
impl GreeterInput for Arc<dyn GreeterHost> {
    fn prepare_greeter_input(&self) -> PreparedGreeterInput {
        PreparedGreeterInput::HostSource(HostSource {
            receiver_key: Arc::as_ptr(self) as *const () as usize,
            descriptors: self.baml_descriptor_bundle(),
            host: self.clone(),
        })
    }
}

// Generated ergonomic methods bind only when the async call starts. The
// default remains runtime-owned.
trait GreeterExt: GreeterInput {
    fn label<'a>(&'a self, ctx: &'a CallContext) -> BoxFuture<'a, Result<String, CallError>> {
        let input = self.prepare_greeter_input();
        Box::pin(async move { ctx.runtime.invoke_label_default(input).await })
    }
}

impl<T: GreeterInput + ?Sized> GreeterExt for T {}

#[allow(non_snake_case)]
async fn Welcome_async<I: GreeterInput + ?Sized>(
    greeter: &I,
    name: String,
    ctx: &CallContext,
) -> Result<(String, GreeterRef), CallError> {
    let view = ctx.runtime.bind(greeter.prepare_greeter_input()).await?;
    let message = view.greet(name).await?;
    Ok((message, view))
}

#[derive(Default)]
struct WelcomeOptions {
    greeter: Option<PreparedGreeterInput>,
}

impl WelcomeOptions {
    // Captures an owned Arc synchronously. There is intentionally no blanket
    // implementation for `&H`: a later BAML call may retain the receiver.
    fn greeter(mut self, value: &(impl GreeterInput + ?Sized)) -> Self {
        self.greeter = Some(value.prepare_greeter_input());
        self
    }

    async fn invoke(&self, name: String, ctx: &CallContext) -> Result<GreeterRef, CallError> {
        let view = ctx
            .runtime
            .bind(self.greeter.as_ref().unwrap().clone())
            .await?;
        assert_eq!(view.greet(name).await?, "Hello, Options");
        Ok(view)
    }
}

struct MyGreeter {
    prefix: String,
    drops: Arc<AtomicUsize>,
}

impl GreeterHost for MyGreeter {
    fn greet<'a>(
        &'a self,
        name: String,
        _ctx: HostCallContext,
    ) -> BoxFuture<'a, Result<String, CallError>> {
        Box::pin(async move { Ok(format!("{}, {}", self.prefix, name)) })
    }
}

impl Drop for MyGreeter {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::SeqCst);
    }
}

fn assert_input<T: GreeterInput>() {}

fn main() {
    assert_input::<GreeterRef>();
    assert_input::<FriendlyGreeterFacade>();
    assert_input::<Arc<MyGreeter>>();
    assert_input::<Arc<dyn GreeterHost>>();
    assert_input::<GreeterImplementation>();

    // Primary API: synchronous implementation factory, no runtime/scope at
    // construction, direct generated-function input, generated default method,
    // and clone-stable receiver identity.
    let primary_drops = Arc::new(AtomicUsize::new(0));
    let primary_runtime = Arc::new(Runtime::new());
    let primary_ctx = CallContext {
        runtime: primary_runtime.clone(),
    };
    let primary = Greeter::implement(MyGreeter {
        prefix: "Primary".to_owned(),
        drops: primary_drops.clone(),
    });
    let primary_clone = primary.clone();
    let (primary_message, primary_retained_by_baml) =
        ready(Welcome_async(&primary, "Ada".to_owned(), &primary_ctx)).unwrap();
    assert_eq!(primary_message, "Primary, Ada");
    assert_eq!(ready(primary.label(&primary_ctx)).unwrap(), "greeter");
    let primary_clone_view =
        ready(primary_runtime.bind(primary_clone.prepare_greeter_input())).unwrap();
    assert_eq!(
        primary_retained_by_baml.registration_id(),
        primary_clone_view.registration_id()
    );
    drop(primary);
    drop(primary_clone);
    drop(primary_clone_view);
    assert_eq!(primary_drops.load(Ordering::SeqCst), 0);
    drop(primary_retained_by_baml);
    assert_eq!(primary_drops.load(Ordering::SeqCst), 1);
    drop(primary_runtime);
    assert_eq!(primary_drops.load(Ordering::SeqCst), 1);

    let drops = Arc::new(AtomicUsize::new(0));
    let runtime = Arc::new(Runtime::new());
    let ctx = CallContext {
        runtime: runtime.clone(),
    };

    let greeter = Arc::new(MyGreeter {
        prefix: "Hello".to_owned(),
        drops: drops.clone(),
    });
    let clone_a = greeter.clone();
    let clone_b = greeter.clone();

    let (message, first_view) = ready(Welcome_async(&greeter, "Ada".to_owned(), &ctx)).unwrap();
    assert_eq!(message, "Hello, Ada");
    assert_eq!(ready(greeter.label(&ctx)).unwrap(), "greeter");

    // Two Arc clones bind to one receiver while its registration is live.
    let view_a = ready(runtime.bind(clone_a.prepare_greeter_input())).unwrap();
    let view_b = ready(runtime.bind(clone_b.prepare_greeter_input())).unwrap();
    assert_eq!(first_view.receiver_key(), view_a.receiver_key());
    assert_eq!(first_view.registration_id(), view_a.registration_id());
    assert_eq!(view_a.registration_id(), view_b.registration_id());

    // The already-erased Arc uses its explicit, disjoint input implementation
    // and still binds to the same receiver/registration.
    let erased: Arc<dyn GreeterHost> = greeter.clone();
    let erased_view = ready(runtime.bind(erased.prepare_greeter_input())).unwrap();
    assert_eq!(first_view.registration_id(), erased_view.registration_id());

    // Expected-interface adaptation may not widen the published witness table
    // for this receiver. A real combined implementation must start from one
    // explicit bundle/factory instead.
    let conflicting_bundle = PreparedGreeterInput::HostSource(HostSource {
        receiver_key: Arc::as_ptr(&erased) as *const () as usize,
        host: erased.clone(),
        descriptors: &GREETER_AND_OTHER,
    });
    assert!(matches!(
        ready(runtime.bind(conflicting_bundle)),
        Err(CallError::DescriptorBundleMismatch)
    ));

    // Optional arguments acquire ownership now and touch the runtime later.
    let options = WelcomeOptions::default().greeter(&greeter);
    drop(erased);
    drop(clone_a);
    drop(clone_b);
    drop(greeter);
    assert_eq!(drops.load(Ordering::SeqCst), 0);

    let retained_by_baml = ready(options.invoke("Options".to_owned(), &ctx)).unwrap();
    assert_eq!(
        retained_by_baml.registration_id(),
        first_view.registration_id()
    );

    // Release ordinary SDK views and the options-owned HostSource. The mock
    // BAML-retained checked view still owns the host receiver.
    drop(first_view);
    drop(view_a);
    drop(view_b);
    drop(erased_view);
    drop(options);
    assert_eq!(drops.load(Ordering::SeqCst), 0);

    drop(retained_by_baml);
    assert_eq!(drops.load(Ordering::SeqCst), 1);
    drop(runtime);
    assert_eq!(drops.load(Ordering::SeqCst), 1);

    println!(
        "PASS: Greeter::implement primary; Arc<H> experiment; runtime-late binding; default label; stable receiver; retained ownership; one final drop"
    );
}
