// A role marker only starts a message directly inside a `prompt` template.
// An untagged string stringifies it, so the marker turns into text like
// `Role { name: "user", metadata: {} }` and every turn lands in one message.
// Interpolating an `ai.Role` into an untagged string is an error, whether the
// role is the value itself or sits in a union, list, map value, or alias.

// The reported bug: building turns with `.map` inside a prompt.
function map_inside_prompt(past: string[], ctx: ai.Context) -> string {
    let render = prompt`
        ${role("system")}
        You are a helpful assistant.

        ${past.map((p) -> {
            `
                ${role("user")}
                ${p}
            `
        })}
    `;
    render(ctx).text()
}

function direct_role(r: ai.Role) -> string {
    `${r} hello`
}

function role_in_union(r: ai.Role | string) -> string {
    `${r}`
}

function role_list(rs: ai.Role[]) -> string {
    `${rs}`
}

function role_map(rs: map<string, ai.Role>) -> string {
    `${rs}`
}

type RoleOrText = ai.Role | string;

function role_alias(r: RoleOrText) -> string {
    `${r}`
}

// Allowed: `${for}` keeps each marker in the prompt template itself.
function for_inside_prompt(past: string[], ctx: ai.Context) -> string {
    let render = prompt`
        ${role("system")}
        You are a helpful assistant.

        ${for (let p in past)}
            ${role("user")}
            ${p}
        ${endfor}
    `;
    render(ctx).text()
}

// Allowed: an explicit `to_string()` asks for the text on purpose.
function explicit_to_string(r: ai.Role) -> string {
    `${r.to_string()}`
}
