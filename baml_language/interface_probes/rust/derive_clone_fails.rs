use std::{marker::PhantomData, sync::Arc};

struct NotClone;

#[derive(Clone)]
struct DecoderRef<O> {
    receiver: Arc<()>,
    output: PhantomData<fn(O) -> O>,
}

fn main() {
    let value = DecoderRef::<NotClone> { receiver: Arc::new(()), output: PhantomData };
    let _ = value.clone();
}
