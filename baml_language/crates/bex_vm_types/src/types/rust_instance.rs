//! Instances of `@@rust_type` classes.
//!
//! A class tagged `@@rust_type` keeps its state in Rust rather than in BAML
//! fields. Its instance is one heap object: the class it belongs to, the type
//! arguments it was built at, and the state itself, which no BAML code can
//! name, read, or reassign. That is what keeps the state tied to exactly one
//! class at exactly its type arguments: nothing can move an `OrderedSet<int>`'s
//! members under an `OrderedSet<string>`, because there is no field to move.
//!
//! The kind of state is one [`RustPayload`] variant per tagged kind, and every
//! behavior a heap value has — tracing, copying, comparing, hashing,
//! rendering, crossing to the host — matches on it exhaustively, so a new kind
//! is a decision at each of them rather than a fallback.

use std::sync::Arc;

use baml_base::RustPayloadKind;

use crate::{BexRustData, HeapPtr};

/// The instance of a `@@rust_type` class.
///
/// The class and type arguments are what [`Instance`](super::Instance) holds;
/// the payload stands in for the fields a plain class would have.
#[derive(Clone, Debug)]
pub struct RustInstance {
    /// The `Object::Class` this is an instance of.
    pub class: HeapPtr,
    /// The class's type arguments, realized at construction, in the class's
    /// parameter order. Empty for a non-generic class.
    pub class_type_args: Box<[crate::RealizedTy]>,
    pub payload: RustPayload,
}

/// The payload type of each kind, named as the kind: what a native reads
/// from, and builds, an instance of a class tagged `@@rust_type("Kind")`.
pub mod payload {
    use std::sync::Arc;

    use crate::OwnedState;

    /// A `baml.hash.DefaultHasher`'s state, the VM's own.
    pub type DefaultHasher = Box<OwnedState<std::hash::DefaultHasher>>;
    /// A `baml.crypto.Sha256`'s compression state, the VM's own.
    pub type Sha256 = Box<OwnedState<sha2::Sha256>>;
    /// A `baml.crypto.Aes256GcmSiv`'s key schedule, which never changes.
    pub type Aes256GcmSiv = Arc<aes_gcm_siv::Aes256GcmSiv>;
    /// A `baml.crypto.Aes128GcmSiv`'s key schedule, which never changes.
    pub type Aes128GcmSiv = Arc<aes_gcm_siv::Aes128GcmSiv>;
    /// A `baml.crypto.ChaCha20Poly1305`'s key, which never changes.
    pub type ChaCha20Poly1305 = Arc<chacha20poly1305::ChaCha20Poly1305>;
    /// A `baml.crypto.XChaCha20Poly1305`'s key, which never changes.
    pub type XChaCha20Poly1305 = Arc<chacha20poly1305::XChaCha20Poly1305>;
    /// A `baml.regex.Regex`'s compiled program, which never changes.
    pub type Regex = Arc<sys_regex::Program>;
    /// A `baml.csv.Record`'s parsed cells, which never change.
    pub type CsvRecord = Arc<crate::CsvRecord>;
    /// A `baml.spawn.Limit`'s admission state, which every holder shares: the
    /// sharing is what a limit is.
    pub type Limit = Arc<crate::LimitInner>;
    /// A `baml.spawn.CancelToken`'s sources, which every holder shares with
    /// the tasks it cancels.
    pub type CancelToken = Arc<crate::cancellation::CancelTokenState>;
    /// A `trace.SpanId`'s recording key, a value compared by what it names.
    /// Boxed to keep every variant pointer-sized.
    pub type SpanId = Box<crate::trace::SpanId>;
    /// A `trace.Options`'s settings, which never change: a builder makes new
    /// options.
    pub type TraceOptions = Arc<crate::trace::TraceOptionsData>;
    /// A `trace.ReservedSpan`'s reservation, whose single attachment every
    /// holder shares.
    pub type ReservedSpan = Arc<crate::trace::ReservedSpanData>;
    /// A `baml.fs.File`: the platform's open file, which every holder shares.
    pub type File = Arc<dyn crate::resource::FileResource>;
    /// A `baml.glob.Glob`: the platform's compiled pattern.
    pub type Glob = Arc<dyn crate::resource::GlobResource>;
    /// A `baml.net.TcpStream`: the platform's connection, which every holder
    /// shares.
    pub type TcpStream = Arc<dyn crate::resource::TcpStreamResource>;
    /// A `baml.net.TcpListener`: the platform's listener, which every holder
    /// shares.
    pub type TcpListener = Arc<dyn crate::resource::TcpListenerResource>;
    /// A `baml.net.UdpSocket`: the platform's socket, which every holder
    /// shares.
    pub type UdpSocket = Arc<dyn crate::resource::UdpSocketResource>;
    /// A `baml.random.Xoshiro256PlusPlus`'s generator state, the VM's own.
    pub type Xoshiro256PlusPlus = Box<OwnedState<rand_xoshiro::Xoshiro256PlusPlus>>;
    /// A `baml.random.ChaCha20`'s generator state, the VM's own.
    pub type ChaCha20 = Box<OwnedState<rand_chacha::ChaCha20Rng>>;
    /// A `reflect.interface.Implementation`'s witness, which never changes:
    /// linking a field builds a new one. It holds heap edges.
    pub type InterfaceWitness = Box<crate::InterfaceWitness>;
    /// A `baml.csv.Reader`'s state, which its reader shares with every copy:
    /// it is tied to the reader's file. Growth is charged when it is locked.
    pub type CsvReader = Arc<crate::MeteredMutex<crate::csv::ReaderState>>;
    /// A `baml.csv.Writer`'s state, shared like a reader's.
    pub type CsvWriter = Arc<crate::MeteredMutex<crate::csv::WriterState>>;
    /// A `reflect.class.Builder`'s state, which every copy of the builder
    /// shares: it is one definition being built.
    pub type ClassBuilder = Arc<crate::MeteredMutex<crate::class_builder::BuilderState>>;
    /// A `baml.sys.Process`'s child: the platform's running process, which
    /// every holder shares.
    pub type ChildProcess = Arc<dyn crate::resource::ChildProcessResource>;
    /// A `baml.sys.ReadPipe`: the platform's pipe, which every holder shares.
    pub type ReadPipe = Arc<dyn crate::resource::ReadPipeResource>;
    /// A `baml.sys.WritePipe`: the platform's pipe, which every holder shares.
    pub type WritePipe = Arc<dyn crate::resource::WritePipeResource>;
}

/// The state behind a `@@rust_type` instance, one variant per kind.
///
/// A variant decides its own ownership, and its type's `Clone` carries the
/// decision, so a deep copy is a clone. State the VM owns is a plain value
/// behind a `Box`, and a copy copies it. State that is immutable, or shared
/// with something outside the VM (a detached task, a host), is an `Arc`, and
/// a copy shares it.
#[derive(Clone)]
pub enum RustPayload {
    /// A value the VM cannot look inside, reached only by downcasting its
    /// `Arc`. Transitional: it stands in for every kind that has no variant
    /// of its own yet, and goes away once each has one.
    Foreign(Arc<dyn BexRustData>),
    DefaultHasher(payload::DefaultHasher),
    Sha256(payload::Sha256),
    Aes256GcmSiv(payload::Aes256GcmSiv),
    Aes128GcmSiv(payload::Aes128GcmSiv),
    ChaCha20Poly1305(payload::ChaCha20Poly1305),
    XChaCha20Poly1305(payload::XChaCha20Poly1305),
    Regex(payload::Regex),
    CsvRecord(payload::CsvRecord),
    Limit(payload::Limit),
    CancelToken(payload::CancelToken),
    SpanId(payload::SpanId),
    TraceOptions(payload::TraceOptions),
    ReservedSpan(payload::ReservedSpan),
    File(payload::File),
    Glob(payload::Glob),
    TcpStream(payload::TcpStream),
    TcpListener(payload::TcpListener),
    UdpSocket(payload::UdpSocket),
    Xoshiro256PlusPlus(payload::Xoshiro256PlusPlus),
    ChaCha20(payload::ChaCha20),
    InterfaceWitness(payload::InterfaceWitness),
    CsvReader(payload::CsvReader),
    CsvWriter(payload::CsvWriter),
    ClassBuilder(payload::ClassBuilder),
    ChildProcess(payload::ChildProcess),
    ReadPipe(payload::ReadPipe),
    WritePipe(payload::WritePipe),
}

impl RustPayload {
    /// The kind this payload is, which the class's `@@rust_type` names.
    pub fn kind(&self) -> RustPayloadKind {
        match self {
            Self::Foreign(_) => RustPayloadKind::Foreign,
            Self::DefaultHasher(_) => RustPayloadKind::DefaultHasher,
            Self::Sha256(_) => RustPayloadKind::Sha256,
            Self::Aes256GcmSiv(_) => RustPayloadKind::Aes256GcmSiv,
            Self::Aes128GcmSiv(_) => RustPayloadKind::Aes128GcmSiv,
            Self::ChaCha20Poly1305(_) => RustPayloadKind::ChaCha20Poly1305,
            Self::XChaCha20Poly1305(_) => RustPayloadKind::XChaCha20Poly1305,
            Self::Regex(_) => RustPayloadKind::Regex,
            Self::CsvRecord(_) => RustPayloadKind::CsvRecord,
            Self::Limit(_) => RustPayloadKind::Limit,
            Self::CancelToken(_) => RustPayloadKind::CancelToken,
            Self::SpanId(_) => RustPayloadKind::SpanId,
            Self::TraceOptions(_) => RustPayloadKind::TraceOptions,
            Self::ReservedSpan(_) => RustPayloadKind::ReservedSpan,
            Self::File(_) => RustPayloadKind::File,
            Self::Glob(_) => RustPayloadKind::Glob,
            Self::TcpStream(_) => RustPayloadKind::TcpStream,
            Self::TcpListener(_) => RustPayloadKind::TcpListener,
            Self::UdpSocket(_) => RustPayloadKind::UdpSocket,
            Self::Xoshiro256PlusPlus(_) => RustPayloadKind::Xoshiro256PlusPlus,
            Self::ChaCha20(_) => RustPayloadKind::ChaCha20,
            Self::InterfaceWitness(_) => RustPayloadKind::InterfaceWitness,
            Self::CsvReader(_) => RustPayloadKind::CsvReader,
            Self::CsvWriter(_) => RustPayloadKind::CsvWriter,
            Self::ClassBuilder(_) => RustPayloadKind::ClassBuilder,
            Self::ChildProcess(_) => RustPayloadKind::ChildProcess,
            Self::ReadPipe(_) => RustPayloadKind::ReadPipe,
            Self::WritePipe(_) => RustPayloadKind::WritePipe,
        }
    }
}

impl RustPayload {
    /// The type of the state this payload holds: a foreign payload's own, or
    /// the variant's payload type. For reporting a payload that is not the
    /// type a caller expected.
    pub fn payload_type_id(&self) -> std::any::TypeId {
        use std::any::TypeId;
        match self {
            Self::Foreign(data) => crate::RustDataArc::payload_type_id(data),
            Self::DefaultHasher(_) => TypeId::of::<payload::DefaultHasher>(),
            Self::Sha256(_) => TypeId::of::<payload::Sha256>(),
            Self::Aes256GcmSiv(_) => TypeId::of::<payload::Aes256GcmSiv>(),
            Self::Aes128GcmSiv(_) => TypeId::of::<payload::Aes128GcmSiv>(),
            Self::ChaCha20Poly1305(_) => TypeId::of::<payload::ChaCha20Poly1305>(),
            Self::XChaCha20Poly1305(_) => TypeId::of::<payload::XChaCha20Poly1305>(),
            Self::Regex(_) => TypeId::of::<payload::Regex>(),
            Self::CsvRecord(_) => TypeId::of::<payload::CsvRecord>(),
            Self::Limit(_) => TypeId::of::<payload::Limit>(),
            Self::CancelToken(_) => TypeId::of::<payload::CancelToken>(),
            Self::SpanId(_) => TypeId::of::<payload::SpanId>(),
            Self::TraceOptions(_) => TypeId::of::<payload::TraceOptions>(),
            Self::ReservedSpan(_) => TypeId::of::<payload::ReservedSpan>(),
            Self::File(_) => TypeId::of::<payload::File>(),
            Self::Glob(_) => TypeId::of::<payload::Glob>(),
            Self::TcpStream(_) => TypeId::of::<payload::TcpStream>(),
            Self::TcpListener(_) => TypeId::of::<payload::TcpListener>(),
            Self::UdpSocket(_) => TypeId::of::<payload::UdpSocket>(),
            Self::Xoshiro256PlusPlus(_) => TypeId::of::<payload::Xoshiro256PlusPlus>(),
            Self::ChaCha20(_) => TypeId::of::<payload::ChaCha20>(),
            Self::InterfaceWitness(_) => TypeId::of::<payload::InterfaceWitness>(),
            Self::CsvReader(_) => TypeId::of::<payload::CsvReader>(),
            Self::CsvWriter(_) => TypeId::of::<payload::CsvWriter>(),
            Self::ClassBuilder(_) => TypeId::of::<payload::ClassBuilder>(),
            Self::ChildProcess(_) => TypeId::of::<payload::ChildProcess>(),
            Self::ReadPipe(_) => TypeId::of::<payload::ReadPipe>(),
            Self::WritePipe(_) => TypeId::of::<payload::WritePipe>(),
        }
    }
}

/// The kind only: a payload's contents (a cipher's key, a hasher's state) are
/// nothing to print.
impl std::fmt::Debug for RustPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("RustPayload").field(&self.kind()).finish()
    }
}

impl RustInstance {
    pub fn new(
        class: HeapPtr,
        class_type_args: Box<[crate::RealizedTy]>,
        payload: RustPayload,
    ) -> Self {
        Self {
            class,
            class_type_args,
            payload,
        }
    }
}

impl RustInstance {
    /// Whether `class` (the declaration at [`Self::class`]) can have this
    /// instance: the class is tagged with the payload's own kind, and the
    /// instance is built at as many type arguments as the class declares.
    /// Checked wherever an instance enters the heap.
    #[must_use]
    pub fn fits(&self, class: &crate::Class) -> bool {
        matches!(class.body, crate::ClassBody::RustType(kind) if kind == self.payload.kind())
            && self.class_type_args.len() == class.generic_param_count
    }
}

impl std::fmt::Display for RustInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<rust instance of {:p}>", self.class.as_ptr())
    }
}
