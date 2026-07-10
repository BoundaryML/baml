class BackgroundOptions { idempotency_key: string? }
class JobToken { provider: string, id: string }
class Pending { retry_after: baml.time.Duration? }
class Failed { error: Failure }
class Cancelled {}

interface Job<T> {
  function poll(self) -> Done<T> | Pending | Failed | Cancelled throws baml.errors.UnknownError
  function cancel(self) -> null throws baml.errors.UnknownError
  function token(self) -> JobToken throws never
  function cleanup(self) -> null throws never
}

interface Background requires Provider {
  function submit<T>(self, request: Request<T>, options: BackgroundOptions) -> Job<T>
    throws baml.errors.UnknownError
  function resume_job<T>(self, token: JobToken) -> Job<T>
    throws baml.errors.UnknownError
}

function submit_background<T>(request: Request<T>, options: BackgroundOptions) -> Job<T>
    throws baml.errors.UnknownError {
  let background = match (request.provider) {
    let capability: Background => capability,
    _ => throw baml.errors.UnknownError {
      data: request.provider,
      message: ["provider cannot submit background work"],
    },
  };
  background.submit<T>(request, options)
}

class Batch<T> { id: string, owner: string }
interface Batching requires Provider {
  function submit_batch<T>(
    self,
    requests: Request<T>[],
    key_of: (Request<T>, int) -> string throws never,
  ) -> Batch<T> throws baml.errors.UnknownError
}

function submit_batch<T>(
  provider: Provider,
  requests: Request<T>[],
  key_of: (Request<T>, int) -> string throws never,
) -> Batch<T> throws baml.errors.UnknownError {
  match (provider) {
    let batching: Batching => batching.submit_batch<T>(requests, key_of),
    _ => throw baml.errors.UnknownError { data: provider, message: ["provider cannot batch"] },
  }
}

class SessionOptions {}
class SessionToken { provider: string, id: string }

interface Session {
  function run<T>(self, request: Request<T>) -> T throws baml.errors.UnknownError
  function run_with_meta<T>(self, request: Request<T>) -> Response<T> throws baml.errors.UnknownError
  function provider(self) -> Provider throws never
  function token(self) -> SessionToken throws never
  function cleanup(self) -> null throws never
}

interface Sessions requires Provider {
  function open_session(self, options: SessionOptions) -> Session throws baml.errors.UnknownError
  function resume_session(self, token: SessionToken) -> Session throws baml.errors.UnknownError
}

function open_session(provider: Provider, options: SessionOptions) -> Session
    throws baml.errors.UnknownError {
  let sessions = match (provider) {
    let capability: Sessions => capability,
    _ => throw baml.errors.UnknownError { data: provider, message: ["provider cannot open sessions"] },
  };
  sessions.open_session(options)
}

interface ForkableSession requires Session {
  function fork(self) -> ForkableSession throws baml.errors.UnknownError
}

class CompactionPolicy { name: string? }
class CompactionResult { changed: bool }
interface CompactableSession requires Session {
  function compact(self, policy: CompactionPolicy) -> CompactionResult throws baml.errors.UnknownError
}

interface Channel {
  function on(self, handler: (string) -> null throws never) -> null throws never
  function send(self, data: string) -> null throws baml.errors.Io
  function close(self) -> null throws never
}

class TranscriptDelta { text: string }
class UserSpeechStarted { played_ms: int }
class LiveClosed {}
type LiveEvent = TranscriptDelta | UserSpeechStarted | LiveClosed
type LiveEventStream = LiveEvent[]

interface Live {
  function events(self) -> LiveEventStream throws never
  function cancel_response(self) -> null throws baml.errors.UnknownError
  function truncate_assistant_audio(self, played_ms: int) -> null throws baml.errors.UnknownError
  function close(self) -> null throws never
  function cleanup(self) -> null throws never
}

interface Realtime requires Provider {
  function open_live<T>(self, request: Request<T>, channel: Channel) -> Live
    throws baml.errors.UnknownError
}

function open_live<T>(request: Request<T>, channel: Channel) -> Live throws baml.errors.UnknownError {
  let realtime = match (request.provider) {
    let capability: Realtime => capability,
    _ => throw baml.errors.UnknownError { data: request.provider, message: ["provider cannot open live sessions"] },
  };
  realtime.open_live<T>(request, channel)
}

class CacheOptions { ttl: baml.time.Duration }
interface Cache {
  function run<T>(self, request: Request<T>) -> T
    throws baml.errors.CallError | baml.errors.UnknownError
  function delete(self) -> null throws baml.errors.UnknownError
  function cleanup(self) -> null throws never
}

interface ManagedCache requires Provider {
  function create_cache(self, content: ChatMessage[], options: CacheOptions) -> Cache
    throws baml.errors.UnknownError
}

function create_cache(provider: Provider, content: ChatMessage[], options: CacheOptions) -> Cache
    throws baml.errors.UnknownError {
  let managed = match (provider) {
    let capability: ManagedCache => capability,
    _ => throw baml.errors.UnknownError { data: provider, message: ["provider cannot create caches"] },
  };
  managed.create_cache(content, options)
}
