// BAML builder processor — claims a bamlBuilds row, resolves the release for its
// sha on GitHub, and marks it ready (the worker then downloads that binary per
// run). Ports services/baml_builder.
//
// Focused port: the actual binary download + upload to the api's blob store is
// deferred (the api's /baml-builds/{id}/binary route isn't built yet); this
// resolves the release and flips the build ready|failed.

class BamlBuildDoc {
    _id: string?,
    sha: string?,
    ref: string?,
    channel: string?,
}

class BuildReadyPatch {
    builtAt: int,
    buildLogTail: string,
}

class BamlBuilderProcessor {
    wid: string,
    svc: ServiceClient,
    github_token: string,
    repo_slug: string,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "baml_builder",
                table: "bamlBuilds",
                claim_value: "queued",
                claim_into: "building",
                claim_field: "status",
                claim_index: "by_status_created",
                lease_ms: 900000,
                heartbeat_ms: 60000,
                poll_backstop_ms: 3000,
                presence_ms: 15000,
                batch: false,
            };
        }

        function service(self) -> ServiceClient {
            return self.svc;
        }

        function worker_id(self) -> string {
            return self.wid;
        }

        function process(self, raw: string) -> int throws ProcessError {
            let build = baml.json.from_string<BamlBuildDoc>(raw) catch (_) {
                _ => {
                    throw ProcessError { message: "bad bamlBuild json" }
                }
            };
            let id = match (build._id) {
                let v: string => v,
                null => {
                    throw ProcessError { message: "build missing _id" }
                },
            };

            let parts = self.repo_slug.split("/");
            let owner = match (parts.at(0)) {
                let v: string => v,
                null => "BoundaryML",
            };
            let repo = match (parts.at(1)) {
                let v: string => v,
                null => "baml",
            };
            let releases = github_releases(self.github_token, owner, repo) catch (e) {
                _ => {
                    throw ProcessError { message: "github releases failed: " + baml.unstable.string(e) }
                }
            };

            let sha = match (build.sha) {
                let v: string => v,
                null => "",
            };
            let found = releases.includes(sha) || sha == "";

            // TODO: download the release asset + PUT to the api blob store; for now just
            // resolve the release and flip the build ready.
            let patch = baml
                .json
                .to_string(
                    BuildReadyPatch {
                        builtAt: baml.sys.now_ms(),
                        buildLogTail: if (found) {
                            "release resolved on GitHub"
                        } else {
                            "release not found for sha"
                        },
                    },
                ) catch (_) {
                _ => "{}"
            };
            let _ = self.svc.update("bamlBuilds", id, patch) catch (_) {
                _ => ""
            };
            let to = if (found) {
                "ready"
            } else {
                "failed"
            };
            let _ = self.svc.transition("bamlBuilds", id, to, "status", "{}", true) catch (e) {
                _ => {
                    throw ProcessError { message: "build transition failed: " + baml.unstable.string(e) }
                }
            };
            return 0;
        }
    }
}
