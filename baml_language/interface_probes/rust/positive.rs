//! Isolated proposed-SDK mechanics. No BAML engine or native ABI is involved.
use std::{
    any::TypeId,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    task::{Context, Poll, Wake, Waker},
};

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

struct Scope {
    open: AtomicBool,
}

struct Receiver {
    id: u64,
    scope: Arc<Scope>,
    dropped: Arc<AtomicUsize>,
}

impl Drop for Receiver {
    fn drop(&mut self) {
        self.dropped.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Clone)]
struct OwnedReceiver(Arc<Receiver>);

// Deliberately separate from a Sized, bidirectional BamlValue codec.
trait ClientInput: Send + Sync {
    fn owned_receiver(&self) -> OwnedReceiver;
}

struct ResponsesClient(OwnedReceiver);
struct ClientRef(OwnedReceiver);
struct StreamingClientRef(OwnedReceiver);

macro_rules! input_impl {
    ($($t:ty),+) => {$(
        impl ClientInput for $t {
            fn owned_receiver(&self) -> OwnedReceiver { self.0.clone() }
        }
    )+};
}
input_impl!(ResponsesClient, ClientRef, StreamingClientRef);

#[derive(Default)]
struct CallOptions {
    client: Option<OwnedReceiver>,
}

impl CallOptions {
    // Local ownership is acquired synchronously; invocation checks revocation.
    fn client(mut self, input: &(impl ClientInput + ?Sized)) -> Self {
        self.client = Some(input.owned_receiver());
        self
    }

    async fn call(&self) -> Result<u64, &'static str> {
        let receiver = &self.client.as_ref().ok_or("no client")?.0;
        if !receiver.scope.open.load(Ordering::SeqCst) {
            return Err("scope closed");
        }
        Ok(receiver.id)
    }
}

struct DecoderRef<O> {
    receiver: OwnedReceiver,
    output: PhantomData<fn(O) -> O>,
}

// A handle clone must not require the method's output type to implement Clone.
impl<O> Clone for DecoderRef<O> {
    fn clone(&self) -> Self {
        Self { receiver: self.receiver.clone(), output: PhantomData }
    }
}

struct NotClone;
struct Resume(String);
struct Invoice(u64);
struct FunctionSpec<O> {
    output: TypeId,
    make: fn() -> O,
    invariant: PhantomData<fn(O) -> O>,
}

impl<O: 'static> FunctionSpec<O> {
    fn new(make: fn() -> O) -> Self {
        Self { output: TypeId::of::<O>(), make, invariant: PhantomData }
    }
}

struct Agent;
struct RunResult<O> {
    value: O,
}
impl Agent {
    async fn run<O: 'static>(&self, spec: &FunctionSpec<O>) -> RunResult<O> {
        assert_eq!(spec.output, TypeId::of::<O>());
        RunResult { value: (spec.make)() }
    }
}

type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;
trait GreeterHost: Send + Sync {
    fn greet<'a>(&'a self, name: String) -> BoxFuture<'a, String>;
}
struct Greeter(String);
impl GreeterHost for Greeter {
    fn greet<'a>(&'a self, name: String) -> BoxFuture<'a, String> {
        Box::pin(async move { format!("{}, {}", self.0, name) })
    }
}

fn main() {
    let scope = Arc::new(Scope { open: AtomicBool::new(true) });
    let dropped = Arc::new(AtomicUsize::new(0));
    let options = {
        let receiver = OwnedReceiver(Arc::new(Receiver {
            id: 17,
            scope: scope.clone(),
            dropped: dropped.clone(),
        }));
        let concrete = ResponsesClient(receiver.clone());
        let existential = ClientRef(receiver.clone());
        let required = StreamingClientRef(receiver.clone());
        let inputs: Vec<&dyn ClientInput> = vec![&concrete, &existential, &required];
        for input in inputs {
            assert_eq!(ready(CallOptions::default().client(input).call()), Ok(17));
        }
        let handle = DecoderRef::<NotClone> { receiver, output: PhantomData };
        let cloned = handle.clone();
        assert_eq!(cloned.receiver.0.id, 17);
        CallOptions::default().client(&concrete)
    };
    assert_eq!(dropped.load(Ordering::SeqCst), 0);
    assert_eq!(ready(options.call()), Ok(17));
    scope.open.store(false, Ordering::SeqCst);
    assert_eq!(ready(options.call()), Err("scope closed"));
    drop(options);
    assert_eq!(dropped.load(Ordering::SeqCst), 1);

    let agent = Agent;
    let resume = FunctionSpec::new(|| Resume("Ada".into()));
    let invoice = FunctionSpec::new(|| Invoice(42));
    let a: RunResult<Resume> = ready(agent.run(&resume));
    let b: RunResult<Invoice> = ready(agent.run(&invoice));
    assert_eq!(a.value.0, "Ada");
    assert_eq!(b.value.0, 42);

    let greeter: Arc<dyn GreeterHost> = Arc::new(Greeter("Hello".into()));
    assert_eq!(ready(greeter.greet("Ada".into())), "Hello, Ada");
    println!("PASS: concrete/ref/required-interface inputs; owned options; closed scope rejection; one final drop; Clone without O: Clone; run output inference; borrowed async dyn host method");
}
