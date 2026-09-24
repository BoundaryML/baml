/**
 * Reads the shape of a state dump value (contract section 2.5) for the state
 * tree: a future with its state, a cancel token, a task group, an enum
 * variant, a map, an array, or a class instance.
 *
 * The contract fixes `kind`, `preview`, `class`, and `children`, and a reader
 * must tolerate a kind that it does not know. The snapshot core renders an
 * enum variant as an opaque value whose preview is `Enum.Variant`. It renders
 * a future as the kind `future` with the preview `#<id> (<state>)` and one
 * child `value` or `error`, a cancel token as the kind `cancel_token` with the
 * preview `<cancel token: not cancelled, any of 2>`, and a task group as the
 * kind `task_group`. Other workers, and the fixtures of earlier phases, use an
 * instance of `Future` or an opaque value. Every rule here therefore reads
 * more than one signal, and a value that matches no rule is shown as it is.
 */

import type { DumpChild, DumpValue, StateDump } from "./protocol";

export type FutureState = "pending" | "resolved" | "failed" | "cancelled";

export type ValueShape =
  | { shape: "future"; state: FutureState | null; type: string | null; id: number | null; settled: DumpChild | null; rest: DumpChild[] }
  | { shape: "cancel_token"; cancelled: boolean | null; sources: number }
  | { shape: "task_group"; summary: string }
  | { shape: "enum"; enumName: string; variant: string }
  | { shape: "map"; size: number | null }
  | { shape: "array"; size: number | null }
  | { shape: "instance"; className: string }
  | { shape: "plain" };

const unquote = (text: string): string => text.replace(/^"(.*)"$/, "$1");

/** The state that a word such as `Ready`, `ErrorPending`, or `baml.future.State.Cancelled` names. */
export function futureStateOf(text: string): FutureState | null {
  const word = unquote(text).toLowerCase();
  // `ErrorPending` is a settled error that nobody has observed yet, so the error words come first.
  if (/cancel/.test(word)) return "cancelled";
  if (/error|fail|panic/.test(word)) return "failed";
  if (/ready|resolv|result|success|done|complete|settled/.test(word)) return "resolved";
  if (/pending|running|unscheduled|waiting/.test(word)) return "pending";
  return null;
}

const child = (value: DumpValue, ...keys: string[]): DumpChild | null =>
  value.children?.find((candidate) => keys.includes(candidate.key.toLowerCase())) ?? null;

const lastSegment = (name: string): string => name.replace(/<.*$/, "").split(".").pop() ?? name;

function lengthOf(value: DumpValue): number | null {
  const stated = /\(len (\d+)\)|\[(\d+) items?\]|^\{(\d+) entr/.exec(value.preview);
  const text = stated?.[1] ?? stated?.[2] ?? stated?.[3];
  return text !== undefined ? Number(text) : (value.children?.length ?? null);
}

export function shapeOf(value: DumpValue): ValueShape {
  const className = value.class ?? "";
  const classTail = lastSegment(className);

  const looksLikeFuture = value.kind === "future" || classTail === "Future" || /^<?\s*future\b/i.test(value.preview);
  if (looksLikeFuture) {
    const stateChild = child(value, "state", "status");
    const state = (stateChild ? futureStateOf(stateChild.value.preview) : null) ?? futureStateOf(value.preview.replace(/future/gi, ""));
    const settled = child(value, "value", "result", "error", "panic");
    const type = /future\s*(<.*>)/i.exec(value.preview)?.[1] ?? /(<.*>)/.exec(className)?.[1] ?? null;
    const id = /^#(\d+)\b/.exec(value.preview)?.[1];
    const rest = (value.children ?? []).filter((candidate) => candidate !== stateChild && candidate !== settled);
    return { shape: "future", state, type: type === null ? null : `Future${type}`, id: id === undefined ? null : Number(id), settled, rest };
  }

  if (value.kind === "cancel_token" || classTail === "CancelToken") {
    const flag = child(value, "cancelled", "is_cancelled", "canceled", "fired");
    const cancelled = flag ? flag.value.preview === "true" : /cancelled|fired/i.test(value.preview) ? !/not cancelled|armed/i.test(value.preview) : null;
    // `CancelToken.any`: the token fires when one of its sources fires.
    const sources = /any of (\d+)/.exec(value.preview)?.[1];
    return { shape: "cancel_token", cancelled, sources: sources === undefined ? 0 : Number(sources) };
  }

  if (value.kind === "task_group") {
    return { shape: "task_group", summary: value.preview.replace(/^<task group\s*/i, "").replace(/>$/, "") };
  }

  if (classTail === "TaskGroup") {
    const parts = ["name", "limit", "active", "queued"].flatMap((key) => {
      const entry = child(value, key, `${key}_count`);
      return entry && entry.value.kind !== "null" ? [`${key} ${unquote(entry.value.preview)}`] : [];
    });
    return { shape: "task_group", summary: parts.join(" · ") };
  }

  // `Tier.Budget` or `baml.future.State.Pending`: the snapshot core names a variant this way.
  const variant = /^((?:[A-Za-z_]\w*\.)*[A-Z]\w*)\.([A-Za-z_]\w*)$/.exec(value.preview);
  if (value.kind === "enum" || (value.kind === "opaque" && variant)) {
    return variant ? { shape: "enum", enumName: variant[1] as string, variant: variant[2] as string } : { shape: "enum", enumName: className, variant: value.preview };
  }

  if (value.kind === "map") return { shape: "map", size: lengthOf(value) };
  if (value.kind === "array") return { shape: "array", size: lengthOf(value) };
  if (value.kind === "instance") return { shape: "instance", className: className || value.preview };
  return { shape: "plain" };
}

/**
 * A one-line rendering of a value for a collapsed row, for example
 * `Money { amount: 300, currency: "EUR" }`. Nested values are abbreviated, and
 * the text is cut at `budget` characters.
 */
export function inlineSummary(value: DumpValue, budget = 72, depth = 0): string {
  const full = fullSummary(value, depth, budget);
  if (full.length <= budget) return full;
  // A nested value that does not fit is named, not cut in the middle.
  if (depth > 0) return compactSummary(value);
  return `${full.slice(0, Math.max(budget - 1, 1))}…`;
}

/** The budget of a value inside another value. */
const NESTED_BUDGET = 48;

function typeName(className: string): string {
  return lastSegment(className) + (/(<.*>)$/.exec(className)?.[1] ?? "");
}

function sizeText(shape: "map" | "array", size: number): string {
  return shape === "map" ? `{${size} ${size === 1 ? "entry" : "entries"}}` : `[${size} ${size === 1 ? "item" : "items"}]`;
}

function compactSummary(value: DumpValue): string {
  const shape = shapeOf(value);
  switch (shape.shape) {
    case "future":
      return `${shape.type ?? "Future"}${shape.id === null ? "" : ` #${shape.id}`} ${shape.state ?? "?"}`;
    case "map":
    case "array":
      return sizeText(shape.shape, shape.size ?? value.children?.length ?? 0);
    case "instance": {
      // An error is worth its message, also when the rest of it does not fit.
      const message = child(value, "message");
      if (message && message.value.kind === "string") {
        const text = `${typeName(shape.className)}: ${unquote(message.value.preview)}`;
        return text.length > NESTED_BUDGET ? `${text.slice(0, NESTED_BUDGET - 1)}…` : text;
      }
      return `${typeName(shape.className)} {…}`;
    }
    default: {
      const text = fullSummary(value, 3, NESTED_BUDGET);
      return text.length > NESTED_BUDGET ? `${text.slice(0, NESTED_BUDGET - 1)}…` : text;
    }
  }
}

function fullSummary(value: DumpValue, depth: number, budget: number): string {
  const shape = shapeOf(value);
  switch (shape.shape) {
    case "future":
    {
      const head = `${shape.type ?? "Future"}${shape.id === null ? "" : ` #${shape.id}`} ${shape.state ?? "?"}`;
      // The value of a settled future gets the room that the head leaves.
      return shape.settled && depth < 1 ? `${head} = ${inlineSummary(shape.settled.value, Math.max(budget - head.length - 3, NESTED_BUDGET), depth + 1)}` : head;
    }
    case "cancel_token":
      return `CancelToken ${shape.cancelled === null ? "" : shape.cancelled ? "cancelled" : "armed"}${shape.sources > 0 ? ` (any of ${shape.sources})` : ""}`.trim();
    case "task_group":
      return `TaskGroup ${shape.summary}`.trim();
    case "enum":
      return `${lastSegment(shape.enumName)}.${shape.variant}`;
    case "map":
    case "array": {
      const size = shape.size ?? value.children?.length ?? 0;
      if (depth >= 1 || !value.children || value.children.length === 0) return sizeText(shape.shape, size);
      const items = value.children.map((entry) =>
        shape.shape === "map" ? `${JSON.stringify(entry.key)}: ${inlineSummary(entry.value, NESTED_BUDGET, depth + 1)}` : inlineSummary(entry.value, NESTED_BUDGET, depth + 1),
      );
      return shape.shape === "map" ? `{ ${items.join(", ")} }` : `[${items.join(", ")}]`;
    }
    case "instance": {
      const name = typeName(shape.className);
      if (depth >= 3 || !value.children || value.children.length === 0) return name;
      return `${name} { ${value.children.map((entry) => `${entry.key}: ${inlineSummary(entry.value, NESTED_BUDGET, depth + 1)}`).join(", ")} }`;
    }
    case "plain":
      return value.preview;
  }
}

/**
 * The threads of a dump in the order the state tree shows them: the root
 * thread first, then the spawned threads by id. The real worker writes the
 * threads in the order in which they parked, which puts the root last when it
 * is the thread that went to sleep.
 */
export function orderedThreads(dump: StateDump): StateDump["threads"] {
  const isRoot = (thread: StateDump["threads"][number]): boolean => typeof thread.parent_thread !== "number";
  return [...dump.threads].sort((a, b) => Number(isRoot(b)) - Number(isRoot(a)) || a.thread - b.thread);
}
