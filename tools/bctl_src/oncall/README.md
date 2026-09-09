# bctl oncall

`bctl oncall` manages our on-call roster. The source of truth is a single file, [`data/schedule.oncall`](./data/schedule.oncall), that holds both the roster (who is in which rotation) and the week-by-week shift assignments.

`oncall-releases` is the team's release rotation and includes everyone: antonio, kai, paulo, sam, vbv, aaron, and avery. It replaces the retired founders and Discord rotations. Shifts start on Fridays using the Pacific calendar date. The release oncaller investigates failed releases, prepares the changelog, and puts out the weekly release.

# Updating `schedule.oncall`

- to swap shifts with someone, edit `schedule.oncall` - [click here to edit in github.dev](https://github.dev/BoundaryML/baml/blob/canary/tools/bctl_src/oncall/data/schedule.oncall)
- to add someone to the oncall roster, add them to `roster_by_rotation` then run `bctl oncall check --fix` 

# `schedule.oncall` file format

The file has three types of data:

- oncall metadata, defined in YAML comments between `BEGIN_ONCALL_METADATA` and `END_ONCALL_METADATA`
- the schedule itself: `YYYY-MM-DD <rotation>=<person> <rotation>=<person> ...`
- `#` indicates comments

# Tooling

- `bctl oncall current [--rotation oncall-releases]` prints the rotation's current assignee from the local schedule using today's Pacific date. Release failure notifications explicitly use `oncall-releases`.
- `bctl oncall remind-release [--channel '#general'] [--post-to-slack]` previews the release and changelog reminder locally, or posts it with a Slack mention when `--post-to-slack` is supplied. The separate [weekly release reminder workflow](../../../.github/workflows/release-reminder.yml) runs every Friday at 8am Pacific, including daylight saving changes, and posts to `#general`.
- `bctl oncall check [--fix]` parses `schedule.oncall` and validates it, ensuring that the file is well-formed
	- runs on every change to `schedule.oncall`
- `bctl oncall notify [--post-to-slack]`
	- post to `#oncall` with a message about who's currently oncall, who was last oncall, and upcoming oncalls
	- runs weekly Friday 9am PT, posting to Slack to notify whoever's oncall (1h drift for PST/PDT is fine)
- `bctl oncall fill-schedule [--post-to-slack]`
	- append weekly shifts to `schedule.oncall`, assigning in a round-robin pattern
