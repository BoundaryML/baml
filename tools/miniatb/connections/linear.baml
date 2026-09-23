function linear_api(query: string, variables: json) -> json {
    let key = baml.env.get("ATB2_LINEAR_API_KEY") ?? baml.env.get("ATB_LINEAR_TOKEN") ?? "";
    require(key != "", "Linear is not configured");
    let response = baml.http.send(
        baml.http.Request {
            method: "POST",
            url: "https://api.linear.app/graphql",
            headers: { "Authorization": key, "Content-Type": "application/json" },
            body: baml.json.stringify({ "query": query, "variables": variables }.to_json()),
        },
        timeout = baml.time.Duration.from_seconds(30),
    );
    require(response.ok(), "Linear request failed");
    let result = baml.json.parse(response.text());
    require(baml.json.path_or<json[]>(result, ".errors", []).length() == 0, "Linear rejected the request");
    baml.json.path<json>(result, ".data")
}

function linear_team() -> string {
    let team = baml.env.get("ATB2_LINEAR_TEAM") ?? "";
    require(team != "", "Linear team is not configured");
    if (team.length() == 36 && team.split("-").length() == 5) {
        return team;
    };
    let data = linear_api(
        "query($key: String!) { teams(filter: { key: { eq: $key } }) { nodes { id } } }",
        { "key": team }.to_json(),
    );
    let teams = baml.json.path<json[]>(data, ".teams.nodes");
    require(teams.length() == 1, "Linear team is ambiguous or missing");
    baml.json.path<string>(teams.at(0) ?? baml.sys.panic("Missing team"), ".id")
}

function export_linear(id: string) -> json {
    let row = load_issue_row(id);
    let directory = data_dir() + "/exports";
    mkdir(directory);
    let temporary = directory + "/" + baml.id.new() + ".tmp";
    let lock = directory + "/" + id + ".lock";
    baml.fs.write(temporary, "exporting");
    defer { baml.fs.remove(temporary); }
    let acquired = baml.sys.exec("/usr/bin/env", safe_args("ln", [temporary, lock]), null);
    require(acquired.exit_code == 0, "An export is already pending; inspect its lock before retrying");
    defer { baml.fs.remove(lock); }
    let team = linear_team();
    let marker = "miniatb: " + dataset() + ": " + id;
    let found = linear_api(
        "query($marker: String!, $team: ID!) { issues(filter: { description: { contains: $marker }, team: { id: { eq: $team } } }, first: 250) { pageInfo { hasNextPage } nodes { id url description } } }",
        { "marker": marker, "team": team }.to_json(),
    );
    require(!baml.json.path<bool>(found, ".issues.pageInfo.hasNextPage"), "Incomplete Linear lookup");
    let matches = baml.json.path<json[]>(found, ".issues.nodes").filter((item) -> {
        baml.json.path_or<string>(item, ".description", "").split("\n").includes(marker)
    });
    require(matches.length() <= 1, "Ambiguous Linear export");
    let description = baml.json.path<string>(row, ".description")
        + "\n\nReproduction and agent sessions: "
        + issue_url(id)
        + "\n\n"
        + marker;
    let fields = {
        "title": baml.json.path<string>(row, ".title").to_json(),
        "description": description.to_json(),
    };
    let existing = matches.at(0);
    let data: json = baml.json.parse("{}");
    let operation = "issueCreate";
    if (existing != null) {
        operation = "issueUpdate";
        data
            = linear_api(
                "mutation($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success issue { url } } }",
                { "id": baml.json.path<string>(existing, ".id"), "input": fields.to_json() }
                    .to_json(),
            );
    } else {
        fields.set("teamId", team.to_json());
        data
            = linear_api(
                "mutation($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { url } } }",
                { "input": fields.to_json() }.to_json(),
            );
    };
    require(baml.json.path<bool>(data, "." + operation + ".success"), "Linear export failed");
    let url = baml.json.path<string>(data, "." + operation + ".issue.url");
    require(url.starts_with("https://linear.app/"), "Invalid Linear issue URL");
    { "url": url, "updated": existing != null }.to_json()
}
