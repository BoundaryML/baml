/// Optional native transport deadlines. Null/non-positive values are unbounded.
class LlmClientOptions {
    request_timeout: root.time.Duration?,
    connect_timeout: root.time.Duration?,
    time_to_first_token_timeout: root.time.Duration?,
    idle_timeout: root.time.Duration?,

    function new() -> LlmClientOptions throws never {
        LlmClientOptions {
            request_timeout: null,
            connect_timeout: null,
            time_to_first_token_timeout: null,
            idle_timeout: null,
        }
    }

    function with_request_timeout(
        self,
        request_timeout: root.time.Duration?,
    ) -> LlmClientOptions throws never {
        self.request_timeout = request_timeout;
        self
    }

    function with_connect_timeout(
        self,
        connect_timeout: root.time.Duration?,
    ) -> LlmClientOptions throws never {
        self.connect_timeout = connect_timeout;
        self
    }

    function with_time_to_first_token_timeout(
        self,
        time_to_first_token_timeout: root.time.Duration?,
    ) -> LlmClientOptions throws never {
        self.time_to_first_token_timeout = time_to_first_token_timeout;
        self
    }

    function with_idle_timeout(
        self,
        idle_timeout: root.time.Duration?,
    ) -> LlmClientOptions throws never {
        self.idle_timeout = idle_timeout;
        self
    }
}
