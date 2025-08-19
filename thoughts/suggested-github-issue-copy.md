# Suggested Copy for GitHub Issue #2339

**Title**: PSA: Future breaking changes to enums in prompt templates

---

In BAML 0.206.0, we're introducing three improvements to enum semantics in templates.

**Let us know if you have a problem with any of these changes** (here or on [Discord](https://boundaryml.com/discord)).

# `{{ enum }}` will now render as the alias

This will be consistent with `ctx.output_format`. If no `@alias` is set, it will continue to render as the enum value.

```jinja
enum Status { Complete @alias("done") }

template MyTemplate(status: Status) #"
    <!-- Old behavior (≤0.205.1): "Status: Complete" -->
    <!-- New behavior (≥0.206.0): "Status: done" -->
    Status: {{ status }}               
"#
```

# You can now use `Enum.Value`.

This will allow rendering specific enum aliases.

```jinja
enum Status { Complete @alias("done") }

template MyTemplate(status: Status) #"
    <!-- New behavior (≥0.206.0): "Status: done" -->
    Status: {{ Status.Complete }}

    <!-- New behavior (≥0.206.0): "Task is complete!" -->
    {% if status == Status.Complete %}
        Task is complete!
    {% endif %}
"#
```

# Comparing enums with strings will be deprecated.

```jinja
enum Status { Complete @alias("done") }

template MyTemplate(status: Status) #"
    <!-- Old behavior (≤0.205.1): "Task is complete!" -->
    <!-- New behavior (≥0.206.0): this will still work, but will be a compiler warning -->
    <!-- Proposed future behavior: compiler error -->
    {% if status == "Complete" %}
        Task is complete!
    {% endif %}
"#
```

## Migration

### If you compare enums with specific strings

Replace string comparisons with the new enum literal syntax:

```jinja
<!-- Before -->
{% if status == "Complete" %}
    Task is complete!
{% endif %}

<!-- After -->
{% if status == Status.Complete %}
    Task is complete!
{% endif %}
```

To preserve backwards compatibility, `status == "Complete"` will still be `true` for the next few releases, but eventually this will become a compiler error. 

To tell your coding agent what needs to be fixed, you can tell it to run `baml-cli generate` and fix the compiler warnings.

### If you compare enums with a user-provided string

If you depend on the ability to compare enum values with arbitrary strings - e.g. if you use dynamic enums - you'll need to migrate to `enum.value_str` (not yet implemented).

```jinja
{% if some_enum_arg.value_str == some_str_arg %}
    Found matching enum value!
{% endif %}
```

# Let us know if any of these are problems for you.

We strongly believe in backwards compatibility and want to make sure all of your prompt
templates continue to work as we improve enums in prompt templates. Please let us know if your use case is not covered by any of the migration paths described here or if you have other concerns about these changes.