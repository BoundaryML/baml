# UDF (User Defined Functions)

## Overview

This library provides a configuration-driven approach to customizing calculations based on input shape. It allows pattern matching and processing of arbitrary JSON bodies from e.g various LLM providers (OpenAI, Anthropic, Gemini, etc.) and applying custom formulas to calculate different results.

Pattern matching is done via BFS, that is, on each function (override), the immediate children array is processed and the first child that matches is selected for overriding, then the matching continues until the path through the override tree is exhausted.

The example used for testing [`sample-prices.yaml`](./sample-prices.yaml) is a good example of what this can be used for. It is a configuration that allows for usage cost calculation given an API shape.

## Key Features

- **Flexible Formulas**: Define custom calculations using expressions and constants
- **Hierarchical Configuration**: Global defaults with provider-specific and conditional overrides
- **Conditional Matching**: Apply different pricing based on model, date ranges, client options, and more

## Configuration Format

The library uses YAML configuration files to define cost calculation rules.

The basic configuration units are called `functions`. Each function defines:

- `match`: Selects the function to run on the given input. Only one function (one path through the override tree) is considered.
- `constants`: Each function (override) is allowed to add to and modify the constant set.
    The deepest matching override is considered for the value of the constant.
- `returns`: Expressions to calculate results of a function. A function can define multiple results, and an override can define more results than its parent function. A configuration is invalid if it does not define any result through any path in the override tree.
- `overrides`: Can be nested. They are partial function definitions that will inherit their parent's definitions unless they override them.


## Expression Language

The configuration uses Jinja2 expressions to match and compute results. Besides the builtin Jinja2 operators and custom BAML Jinja filters, it defines one more filter:

- `<date string> | date_between(<date start>, <date end>)`: `bool`. Whether the date to the left of the filter happens between start & end. `chrono::NaiveDate` is used to parse them.

### Problem with `if` branches
Consider the Jinja expression:
```jinja
raw.output_tokens_details.cached_tokens if raw.output_token_details else 1
```

Analysis from `minijinja` yields that the following paths are not statically known (they are not `set` variables):
```notrust
raw.output_tokens_details
raw.output_tokens_details.cached_tokens
```

We currently only have the last piece of information: we don't know how they are used, since
we're not doing any manual AST analysis. We find out that `raw` does not have the field
`output_tokens_details`.  Since we have to set `cached_tokens` to zero, we conservatively set
`raw.output_tokens_details` to a map, ending with `raw.output_tokens_details = { cached_tokens
=  0 }`. This makes `raw.output_tokens_details` not an empty map, and thus the `if
raw.output_token_details` branch yields the wrong value.
