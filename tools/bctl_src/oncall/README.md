# bctl oncall

`bctl oncall` manages our on-call roster. The source of truth is a single file, [`data/schedule.oncall`](./data/schedule.oncall), that holds both the roster (who is in which rotation) and the week-by-week shift assignments.

# Updating `schedule.oncall`

- to swap shifts with someone, edit `schedule.oncall` - [click here to edit in github.dev](https://github.dev/BoundaryML/baml/blob/canary/tools/bctl_src/oncall/data/schedule.oncall)
- to add someone to the oncall roster, add them to `roster_by_rotation` then run `bctl oncall check --fix` 

# `schedule.oncall` file format

The file has three types of data:

- oncall metadata, defined in YAML comments between `BEGIN_ONCALL_METADATA` and `END_ONCALL_METADATA`
- the schedule itself: `YYYY-MM-DD <rotation>=<person> <rotation>=<person> ...`
- `#` indicates comments

# Tooling

- `bctl oncall current` prints the current `oncall-releases` assignee from the local schedule using today's Pacific date.
- `bctl oncall check [--fix]` parses `schedule.oncall` and validates it, ensuring that the file is well-formed
	- runs on every change to `schedule.oncall`
- `bctl oncall notify [--post-to-slack]`
	- post to `#general` with a message about who's currently oncall, who was last oncall, and upcoming oncalls, asking the current oncaller to use their agent to prepare the next release, prepare the changelog, and thank external contributors after the changelog is out
	- runs weekly Friday 8am Pacific, including daylight saving changes
- `bctl oncall fill-schedule [--post-to-slack]`
	- append weekly shifts to `schedule.oncall`, assigning in a round-robin pattern
