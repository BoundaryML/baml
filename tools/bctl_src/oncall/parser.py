"""Parse and emit the oncall schedule file (hybrid YAML-in-comments + DSL body)."""

from __future__ import annotations

import datetime
import re
from dataclasses import dataclass, field
from typing import Optional

import yaml


BEGIN_MARKER = "# BEGIN_ONCALL_METADATA"
END_MARKER = "# END_ONCALL_METADATA"


@dataclass(frozen=True)
class SlackConfig:
    notification_channel: str


@dataclass(frozen=True)
class Roster:
    by_rotation: dict[str, list[str]]

    def rotations_in_order(self) -> list[str]:
        return [r for r in self.by_rotation.keys() if r != "none"]

    def by_human(self) -> dict[str, list[str]]:
        out: dict[str, list[str]] = {}
        for rot, people in self.by_rotation.items():
            for p in people:
                out.setdefault(p, []).append(rot)
        return dict(sorted(out.items()))

    def members(self) -> set[str]:
        out: set[str] = set()
        for people in self.by_rotation.values():
            out.update(people)
        return out


@dataclass(frozen=True)
class ShiftLine:
    date: datetime.date
    assignments: dict[str, str]
    trailing_comment: Optional[str] = None


@dataclass(frozen=True)
class ScheduleFile:
    slack_config: SlackConfig
    roster: Roster
    shifts: list[ShiftLine]
    free_comments: list[tuple[int, str]] = field(default_factory=list)
    declared_by_human: Optional[dict[str, list[str]]] = None


def _strip_comment_prefix(line: str) -> str:
    if line.startswith("# "):
        return line[2:]
    if line.startswith("#"):
        return line[1:]
    return line


def _denull_key(d: dict, key: str = "none") -> None:
    if None in d:
        d[key] = d.pop(None)


def parse(text: str) -> ScheduleFile:
    lines = text.splitlines()

    metadata_start: Optional[int] = None
    metadata_end: Optional[int] = None
    for i, line in enumerate(lines):
        s = line.strip()
        if s == BEGIN_MARKER:
            if metadata_start is not None:
                raise ValueError(f"L{i + 1}: duplicate {BEGIN_MARKER}")
            metadata_start = i
        elif s == END_MARKER:
            if metadata_end is not None:
                raise ValueError(f"L{i + 1}: duplicate {END_MARKER}")
            metadata_end = i

    if metadata_start is None:
        raise ValueError(f"missing {BEGIN_MARKER}")
    if metadata_end is None:
        raise ValueError(f"missing {END_MARKER}")
    if metadata_end < metadata_start:
        raise ValueError(f"{END_MARKER} appears before {BEGIN_MARKER}")

    metadata_body_lines = lines[metadata_start + 1:metadata_end]
    yaml_src = "\n".join(_strip_comment_prefix(l) for l in metadata_body_lines)
    try:
        meta = yaml.safe_load(yaml_src) or {}
    except yaml.YAMLError as e:
        raise ValueError(f"metadata is not valid YAML: {e}")

    if not isinstance(meta, dict):
        raise ValueError("metadata must be a YAML mapping")

    allowed = {"slack_config", "roster_by_rotation", "roster_by_human"}
    unknown = set(meta.keys()) - allowed
    if unknown:
        raise ValueError(f"unknown metadata top-level keys: {sorted(unknown)}")

    sc = meta.get("slack_config") or {}
    if not isinstance(sc, dict):
        raise ValueError("slack_config must be a mapping")
    channel = sc.get("notification_channel")
    if not isinstance(channel, str) or not channel:
        raise ValueError("slack_config.notification_channel is required")
    slack_config = SlackConfig(notification_channel=channel)

    by_rot_raw = meta.get("roster_by_rotation") or {}
    if not isinstance(by_rot_raw, dict):
        raise ValueError("roster_by_rotation must be a mapping")
    _denull_key(by_rot_raw, "none")
    by_rotation: dict[str, list[str]] = {}
    for k, v in by_rot_raw.items():
        if not isinstance(k, str):
            raise ValueError(f"rotation names must be strings, got {k!r}")
        if not isinstance(v, list) or not all(isinstance(x, str) for x in v):
            raise ValueError(f"rotation {k!r} must be a list of strings")
        by_rotation[k] = list(v)

    declared_by_human_raw = meta.get("roster_by_human")
    declared_by_human: Optional[dict[str, list[str]]] = None
    if declared_by_human_raw is not None:
        if not isinstance(declared_by_human_raw, dict):
            raise ValueError("roster_by_human must be a mapping if present")
        declared_by_human = {}
        for k, v in declared_by_human_raw.items():
            if not isinstance(k, str):
                raise ValueError("roster_by_human keys must be strings")
            if not isinstance(v, list):
                raise ValueError(f"roster_by_human[{k!r}] must be a list of strings")
            normalized: list[str] = []
            for item in v:
                if item is None:
                    normalized.append("none")
                elif isinstance(item, str):
                    normalized.append(item)
                else:
                    raise ValueError(f"roster_by_human[{k!r}] entries must be strings")
            declared_by_human[k] = normalized

    roster = Roster(by_rotation=by_rotation)

    shifts: list[ShiftLine] = []
    free_comments: list[tuple[int, str]] = []

    body_start = metadata_end + 1
    shift_pattern = re.compile(r"^(\d{4}-\d{2}-\d{2})\s+(.+)$")
    for offset, raw in enumerate(lines[body_start:]):
        lineno = body_start + offset + 1
        stripped = raw.strip()
        if not stripped:
            continue
        if stripped.startswith("#"):
            free_comments.append((len(shifts), stripped))
            continue
        m = shift_pattern.match(stripped)
        if not m:
            raise ValueError(f"L{lineno}: invalid shift line: {raw!r}")
        date_str, rest = m.group(1), m.group(2)
        try:
            date = datetime.date.fromisoformat(date_str)
        except ValueError:
            raise ValueError(f"L{lineno}: invalid date: {date_str!r}")
        assignments: dict[str, str] = {}
        for tok in rest.split():
            if "=" not in tok:
                raise ValueError(f"L{lineno}: invalid token {tok!r}")
            k, _, v = tok.partition("=")
            if not k:
                raise ValueError(f"L{lineno}: invalid pair {tok!r}")
            if k in assignments:
                raise ValueError(f"L{lineno}: duplicate rotation {k!r}")
            assignments[k] = v
        shifts.append(ShiftLine(date=date, assignments=assignments))

    return ScheduleFile(
        slack_config=slack_config,
        roster=roster,
        shifts=shifts,
        free_comments=free_comments,
        declared_by_human=declared_by_human,
    )


def _format_metadata_lines(sched: ScheduleFile) -> list[str]:
    lines: list[str] = []
    lines.append("slack_config:")
    lines.append(f'  notification_channel: "{sched.slack_config.notification_channel}"')
    lines.append("")
    lines.append("roster_by_rotation:")
    for rot, people in sched.roster.by_rotation.items():
        names = ", ".join(people)
        lines.append(f"  {rot}: [{names}]")
    lines.append("")
    lines.append("roster_by_human:")
    for human, rots in sched.roster.by_human().items():
        names = ", ".join(rots)
        lines.append(f"  {human}: [{names}]")
    return lines


def _emit_metadata_block(sched: ScheduleFile) -> str:
    inner = _format_metadata_lines(sched)
    wrapped = [BEGIN_MARKER]
    for l in inner:
        wrapped.append(f"# {l}" if l else "#")
    wrapped.append(END_MARKER)
    return "\n".join(wrapped)


def _emit_body(sched: ScheduleFile) -> str:
    rotations = sched.roster.rotations_in_order()
    col_widths: dict[str, int] = {}
    for rot in rotations:
        maxlen = len(f"{rot}=")
        for s in sched.shifts:
            val = s.assignments.get(rot, "")
            if val:
                maxlen = max(maxlen, len(f"{rot}={val}"))
        col_widths[rot] = maxlen

    comments_by_idx: dict[int, list[str]] = {}
    for idx, c in sched.free_comments:
        comments_by_idx.setdefault(idx, []).append(c)

    out: list[str] = []
    for i, shift in enumerate(sched.shifts):
        for c in comments_by_idx.get(i, []):
            out.append(c)
        pairs: list[str] = []
        for j, rot in enumerate(rotations):
            val = shift.assignments.get(rot, "")
            pair = f"{rot}={val}"
            if j < len(rotations) - 1:
                pair = pair.ljust(col_widths[rot])
            pairs.append(pair)
        out.append(f"{shift.date.isoformat()} {' '.join(pairs)}")
    for c in comments_by_idx.get(len(sched.shifts), []):
        out.append(c)
    return "\n".join(out)


def emit(sched: ScheduleFile) -> str:
    metadata = _emit_metadata_block(sched)
    body = _emit_body(sched)
    if body:
        return f"{metadata}\n\n{body}\n"
    return f"{metadata}\n"
