mod baml {
    // =====================================================================
    // Array methods
    // =====================================================================
    struct Array<T> {
        fn length(self: Array<T>) -> i64;
        fn push(self: mut Array<T>, item: T);
        fn at(self: Array<T>, index: i64) -> Result<T>;
        fn concat(self: Array<T>, other: Array<T>) -> Array<T>;
    }

    // =====================================================================
    // String methods
    // =====================================================================
    struct String {
        fn length(self: String) -> i64;
        fn toLowerCase(self: String) -> String;
        fn toUpperCase(self: String) -> String;
        fn trim(self: String) -> String;
        fn includes(self: String, search: String) -> bool;
        fn startsWith(self: String, prefix: String) -> bool;
        fn endsWith(self: String, suffix: String) -> bool;
        #[uses(vm)]
        fn split(self: String, delimiter: String) -> Array<String>;
        fn substring(self: String, start: i64, end: i64) -> String;
        fn replace(self: String, search: String, replacement: String) -> String;
    }

    // =====================================================================
    // Map methods
    // =====================================================================
    struct Map<K, V> {
        fn length(self: Map<K, V>) -> i64;
    }
    // Map.has only works on string-keyed maps, so we define it separately
    // with only V as generic (String in the signature is the concrete type)
    struct Map<V> {
        fn has(self: Map<String, V>, key: String) -> bool;
    }

    // =====================================================================
    // Free functions
    // =====================================================================
    #[uses(vm)]
    fn deep_copy<T>(value: T) -> Result<T>;
    #[uses(vm)]
    fn deep_equals<T>(a: T, b: T) -> bool;

    mod unstable {
        #[uses(vm)]
        fn string<T>(value: T) -> Result<String>;
    }

    // =====================================================================
    // Math functions
    // =====================================================================
    mod math {
        fn trunc(value: f64) -> i64;
    }

    // =====================================================================
    // Media methods
    // =====================================================================
    struct Media {
        fn as_url(self: Media) -> Option<String>;
        fn as_base64(self: Media) -> Option<String>;
        fn as_file(self: Media) -> Option<String>;
        fn mime_type(self: Media) -> Option<String>;
    }

    // =====================================================================
    // Filesystem operations
    // =====================================================================
    mod fs {
        #[builtin]
        struct File {
            private _handle: ResourceHandle,
            #[sys_op]
            #[throws(Io)]
            fn read(self: File) -> String;
            #[sys_op]
            #[throws(Io)]
            fn close(self: File);
        }

        #[sys_op]
        #[throws(Io)]
        fn open(path: String) -> File;
    }

    // =====================================================================
    // System operations
    // =====================================================================
    mod sys {
        /// Execute a shell command and return stdout.
        #[sys_op]
        #[throws(Io)]
        fn shell(command: String) -> String;

        /// Sleep for the given number of milliseconds.
        #[sys_op]
        #[throws(Io)]
        fn sleep(delay_ms: i64);

        /// Abort execution with an error message.
        #[sys_op]
        #[panics(HostPanic)]
        fn panic(message: String);
    }

    // =====================================================================
    // Network operations
    // =====================================================================
    mod net {
        #[builtin]
        struct Socket {
            private _handle: ResourceHandle,
            /// Read data from the socket as a string.
            #[sys_op]
            #[throws(Io, Timeout)]
            fn read(self: Socket) -> String;
            /// Close the socket.
            #[sys_op]
            #[throws(Io)]
            fn close(self: Socket);
        }

        /// Connect to a TCP address (host:port).
        #[sys_op]
        #[throws(Io, Timeout)]
        fn connect(addr: String) -> Socket;
    }

    // =====================================================================
    // HTTP operations
    // =====================================================================
    mod http {
        /// An HTTP request to be sent.
        #[builtin]
        struct Request {
            method: String,
            url: String,
            headers: Map<String, String>,
            body: String,
        }

        #[builtin]
        struct Response {
            private _handle: ResourceHandle,
            status_code: i64,
            headers: Map<String, String>,
            url: String,
            /// Get response body as text (consumes body).
            #[sys_op]
            #[throws(Io)]
            fn text(self: Response) -> String;
            /// Check if status is 2xx.
            #[sys_op]
            #[throws(Io)]
            fn ok(self: Response) -> bool;
        }

        /// Fetch a URL via HTTP GET.
        #[sys_op]
        #[throws(Io, Timeout)]
        fn fetch(url: String) -> Response;

        /// Send an HTTP request and return the response.
        #[sys_op]
        #[throws(Io, Timeout)]
        fn send(request: Request) -> Response;
    }

    // =====================================================================
    // LLM operations
    // =====================================================================
    mod llm {
        /// Prompt AST - a structured prompt for LLM calls.
        /// Opaque: stored as a dedicated heap variant, not as Instance.
        #[builtin]
        #[opaque]
        struct PromptAst {}

        /// The type of an LLM client (primitive, fallback, or round-robin).
        #[builtin]
        enum ClientType {
            Primitive,
            Fallback,
            RoundRobin,
        }

        /// A retry policy for LLM calls.
        #[builtin]
        struct RetryPolicy {
            max_retries: i64,
            initial_delay_ms: i64,
            multiplier: f64,
            max_delay_ms: i64,
        }

        /// An LLM client (primitive, fallback, or round-robin).
        /// Built by get_client from compiler metadata.
        /// Complex fields: accessor/owned codegen is skipped (written manually).
        #[builtin]
        struct Client {
            name: String,
            client_type: ClientType,
            sub_clients: Array<Client>,
            retry: Option<RetryPolicy>,
        }

        /// A primitive LLM client (single provider, fully resolved).
        /// Options have been evaluated (env vars resolved, expressions computed).
        #[builtin]
        struct PrimitiveClient {
            name: String,
            provider: String,
            default_role: String,
            allowed_roles: Vec<String>,
            options: Map<String, Unknown>,

            /// Render a Jinja template with the given arguments.
            /// Returns a structured PromptAst that can be sent to an LLM.
            #[sys_op]
            #[throws(RenderPrompt)]
            fn render_prompt(self: PrimitiveClient, template: String, args: Map<String, Unknown>) -> PromptAst;

            /// Specialize a prompt for this client's provider.
            /// Applies provider-specific transformations (message merging, system prompt
            /// consolidation, metadata filtering).
            #[sys_op]
            #[throws(RenderPrompt, LlmClient)]
            fn specialize_prompt(self: PrimitiveClient, prompt: PromptAst) -> PromptAst;

            /// Build an HTTP request from a specialized prompt.
            /// Creates a provider-specific HTTP request ready to be sent.
            #[sys_op]
            #[throws(LlmClient)]
            fn build_request(self: PrimitiveClient, prompt: PromptAst) -> Request;

            /// Parse an HTTP response into a BAML value.
            /// Interprets the provider-specific response format and parses the output.
            #[sys_op]
            #[throws(LlmClient)]
            fn parse(self: PrimitiveClient, http_response_body: String, type_def: Type) -> Any;
        }

        /// Get the Jinja template for an LLM function.
        #[sys_op]
        #[throws(InvalidArgument)]
        #[uses(engine_ctx)]
        fn get_jinja_template(function_name: String) -> String;

        /// Build a PrimitiveClient from evaluated options.
        /// Called after options have been evaluated by bytecode.
        #[sys_op]
        #[throws(InvalidArgument)]
        fn build_primitive_client(
            name: String,
            provider: String,
            default_role: String,
            allowed_roles: Array<String>,
            options: Map<String, Unknown>
        ) -> PrimitiveClient;

        /// Get a Client tree for an LLM function.
        /// Returns a Client with type, sub-clients, and retry policy.
        #[sys_op]
        #[throws(InvalidArgument)]
        #[uses(engine_ctx)]
        fn get_client(function_name: String) -> Client;

        /// Get the resolve function for a client by name.
        /// Returns a function that resolves to a PrimitiveClient when called.
        #[sys_op]
        #[throws(InvalidArgument)]
        #[uses(engine_ctx)]
        fn resolve_client(client_name: String) -> fn() -> PrimitiveClient;

        /// Get the next round-robin index for a client.
        /// Returns the current counter value and increments it atomically.
        #[sys_op]
        #[throws(InvalidArgument)]
        #[uses(engine_ctx)]
        fn round_robin_next(client_name: String) -> i64;

        /// Peek the current round-robin index for a client.
        /// Returns the current counter value without incrementing it.
        #[sys_op]
        #[throws(InvalidArgument)]
        #[uses(engine_ctx)]
        fn round_robin_peek(client_name: String) -> i64;

        /// Get the return type for an LLM function.
        /// Returns a Type value that can be passed to `parse()`.
        #[sys_op]
        #[throws(InvalidArgument)]
        #[uses(engine_ctx)]
        fn get_return_type(function_name: String) -> Type;
    }
}

mod env {
    #[sys_op]
    #[throws(Io)]
    fn get(key: String) -> Option<String>;
    #[sys_op]
    #[throws(Io)]
    #[panics(HostPanic)]
    fn get_or_panic(key: String) -> String;
}
