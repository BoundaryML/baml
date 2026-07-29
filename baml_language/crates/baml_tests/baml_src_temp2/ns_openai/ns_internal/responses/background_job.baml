// OpenAI Responses background mode is a remote Job<T>: submission returns an
// opaque response ID, polling may happen in another process, and only the token
// plus a separately configured provider are needed to resume it.

class BackgroundJob<T> {
    owner: root.openai.Responses,
    id: string,
    last_status: root.ai.jobs.JobStatus,
    polls: int,

    // function cleanup(self) -> void throws never {
    //   if (self.last_status == root.ai.jobs.JobStatus.Pending) {
    //     self.cancel() catch (e) { _ => null };
    //   }
    // }

    implements root.ai.jobs.Job<T> {
        function status(self) -> root.ai.jobs.JobStatus throws never {
            self.last_status
        }

        function poll(
            self,
        ) -> root.ai.ResponseWithMetadata<T>? throws root.ai.Failure | baml.errors.UnknownError {
            if (self.last_status == root.ai.jobs.JobStatus.Cancelled) {
                return null;
            }
            self.polls = self.polls + 1;
            //# Fetch the durable provider response by opaque ID
            let request = _authenticated_request(
                self.owner,
                "GET",
                (self.owner.base_url ?? "https://api.openai.com/v1") + "/responses/" + self.id,
                _request_headers(self.owner),
                "",
            );
            let response = baml.http.send(request) catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "background poll failed")
                },
            };
            let body = response.text() catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "background poll body failed")
                },
            };
            if (!response.ok()) {
                throw root.ai.internal.classify_http("openai", response.status_code, body);
            }
            let raw = baml.json.parse(body) catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "background poll JSON failed")
                },
            };
            let remote_status = baml.json.path_or(raw, ".status", "");
            if (remote_status == "cancelled") {
                self.last_status = root.ai.jobs.JobStatus.Cancelled;
                return null;
            }
            if (remote_status == "failed" || remote_status == "incomplete") {
                self.last_status = root.ai.jobs.JobStatus.Failed;
                throw baml.errors.UnknownError.with_message<never>(raw, "background response " + remote_status);
            }
            if (remote_status != "completed") {
                self.last_status = root.ai.jobs.JobStatus.Pending;
                return null;
            }
            //## Parse the same native result-tool or SAP mode selected at submission
            let value = _parse_response_value<T>(raw, body, _uses_sap(self.owner));
            self.last_status = root.ai.jobs.JobStatus.Complete;
            root.ai.ResponseWithMetadata<T> {
                value: value,
                metadata: root.ai.ResponseMetadata {
                    provider: self.owner.name(),
                    model: baml.json.path_or<string?>(raw, ".model", self.owner.model),
                    request_id: self.id,
                    finish_reason: remote_status,
                    usage: root.ai.Usage {
                        input_tokens: baml.json.path_or(raw, ".usage.input_tokens", 0),
                        output_tokens: baml.json.path_or(raw, ".usage.output_tokens", 0),
                        cached_input_tokens: baml
                            .json
                            .path_or<int?>(raw, ".usage.input_tokens_details.cached_tokens", null),
                        reasoning_tokens: baml
                            .json
                            .path_or<int?>(
                                raw,
                                ".usage.output_tokens_details.reasoning_tokens",
                                null,
                            ),
                        cost_usd: null,
                    },
                    attributes: { "polls": self.polls, "background": true },
                    raw: raw,
                },
                conversation: null,
            }
        }

        function cancel(self) -> null throws root.ai.Failure | baml.errors.UnknownError {
            if (self.last_status == root.ai.jobs.JobStatus.Cancelled) {
                return null;
            }
            //# Cancel remote work without discarding its resume coordinates
            let request = _authenticated_request(
                self.owner,
                "POST",
                (self.owner.base_url ?? "https://api.openai.com/v1")
                    + "/responses/"
                    + self.id
                    + "/cancel",
                _request_headers(self.owner),
                "{}",
            );
            let response = baml.http.send(request) catch (e) {
                _ => {
                    throw baml.errors.UnknownError.with_message<never>(e, "background cancel failed")
                },
            };
            let body = response.text() catch (e) {
                _ => ""
            };
            if (!response.ok() && self.last_status != root.ai.jobs.JobStatus.Complete) {
                throw root.ai.internal.classify_http("openai", response.status_code, body);
            }
            self.last_status = root.ai.jobs.JobStatus.Cancelled;
            null
        }

        function token(self) -> root.ai.jobs.JobToken throws never {
            root.ai.jobs.JobToken { provider: self.owner.name(), id: self.id, sealed: null }
        }
    }

    implements root.ai.Resource {}
}
