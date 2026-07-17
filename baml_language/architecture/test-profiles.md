# Test IDs and profiles

`baml test` gives every test one canonical, project-local id. The id begins
with `root`, uses `.` for BAML namespaces or function ownership, and uses `::`
for testset nesting:

```text
root::smoke
root.orders::integration::creates_order
root.orders.ChargeCard::declined_card
```

Run `baml test --list` to see the ids in a project. The values it prints can be
used directly with `-i`/`--include` and `-x`/`--exclude`. Selectors are
case-sensitive. A value without `*` matches anywhere in the full canonical id.
A value containing `*` is instead an anchored full-ID glob; `*` can cross `::`
boundaries, while every other character, including `/`, `?`, and `[`, is
literal:

```sh
baml test -i "root.orders::*"
baml test -i "::integration::"
baml test -i "hello" -x "::flaky::"
baml test -i "root.orders::*hello*"
```

Execution uses those same IDs in result lines:

```text
PASS root.orders::integration::creates_order
FAIL root.orders::integration::declines_bad_card
```

If a testset runner tolerates a failing leaf, it is printed as `TOLERATED`
with its canonical ID. Runner-level verdicts use an explicit `AGGREGATE PASS`
or `AGGREGATE FAIL` label; an aggregate glob is not presented as a test ID.

## Saving common invocations

A test profile is a named preset argument vector for `baml test`. It does not
add a new test classification system: names such as `regular` and `integration`
are chosen by the project, and the arguments use the same syntax documented by
`baml test --help`. Profile names are case-sensitive.

```toml
[test]
default = "regular"

[test.profiles.regular]
args = ["-x", "::integration::"]

[test.profiles.integration]
args = ["-i", "::integration::"]
```

With that configuration:

```sh
baml test
# uses the regular profile

baml test --profile integration
# uses the integration profile

baml test --profile integration -i "hello"
# integration tests AND tests whose full id contains hello

baml test --no-profile
# bypasses the configured default profile
```

Think of a profile as preset command-line arguments. For selection, BAML keeps
the preset and explicit arguments as two layers so they compose predictably:
repeated `-i` values within one layer mean OR, while an explicit command-line
filter narrows the profile's candidates. Thus adding `-i` to a saved invocation
cannot accidentally broaden it.

Within either layer, exclusions always win. With no includes, every
non-excluded test is selected. If `[test].default` is absent, plain `baml test`
runs every test. Profile and CLI layers are ANDed, which also lets BAML avoid
executing a lazy testset collector when either layer proves none of its
descendants can be selected.

`args` must be a TOML array, not a shell string. There is no shell expansion or
platform-dependent quoting:

```toml
# Correct
args = ["-i", "root.orders::*", "-x", "*::flaky::*", "--color", "never"]

# Invalid
args = "-i 'root.orders::*'"
```

Profile arguments cannot contain `--profile`, `--no-profile`, `--from`, or
`--help`, because those options determine how the profile itself is resolved.
Other options shown by `baml test --help` use the same parser. Explicit CLI
scalar values, such as `--color`, override profile scalar values.

## Separator migration

Canonical hierarchy always uses `::`. An unambiguous old exact selector such as
`integration::nested/case` produces an error suggesting
`integration::nested::case`. Literal `/` remains legal in names, and wildcard
selectors such as `*::path/to/case` are accepted because BAML cannot infer
whether the slash was intended as hierarchy or literal data. `baml test --list`
is the source of truth when migrating ambiguous selectors.
