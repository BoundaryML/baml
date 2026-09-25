class PosthogState {
    project: string,
    host: string,
    after: string,
    uuid: string,
    until: string?,
}

function posthog_state(project: string, host: string, directory: string? = null) -> PosthogCursor {
    let storage = directory ?? data_dir();
    let path = storage + "/posthog.json";
    mkdir(storage);
    let now = baml.time.Instant.now();
    let state = if (baml.fs.exists(path)) {
        read_json<PosthogState>(path)
    } else {
        PosthogState {
            project: project,
            host: host,
            after: (now - baml.time.Duration.from_seconds(3600)).to_string(),
            uuid: "",
            until: null,
        }
    };
    if (state.project != project || state.host != host) {
        state.project = project;
        state.host = host;
        state.after = (now - baml.time.Duration.from_seconds(3600)).to_string();
        state.uuid = "";
        state.until = null;
    }
    if (state.until == null) {
        state.until = now.to_string();
    }
    write_json(path, state.to_json());
    PosthogCursor { after: state.after, uuid: state.uuid, until: state.until ?? "" }
}

function posthog_checkpoint(checkpoint: json, directory: string? = null) -> void {
    let path = (directory ?? data_dir()) + "/posthog.json";
    let state = read_json<PosthogState>(path);
    let until = baml.json.path<string>(checkpoint, ".until");
    require(state.until == until, "PostHog scan changed");
    if (baml.json.path<bool>(checkpoint, ".complete")) {
        state.after
            = (baml.time.Instant.parse(until) - baml.time.Duration.from_seconds(300)).to_string();
        state.uuid = "";
        state.until = null;
    } else {
        state.after = baml.json.path<string>(checkpoint, ".after");
        state.uuid = baml.json.path<string>(checkpoint, ".uuid");
    }
    write_json(path, state.to_json());
}
