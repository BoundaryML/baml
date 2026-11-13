"use client";

// The forms on a proposal's decisions and issues pages. Each submits to a server action in
// actions.ts and, when refused, shows why and keeps what was typed.
import { useActionState, useEffect, useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Textarea } from "@/components/ui/textarea";
import type { FormState } from "../actions";

type Action = (previous: FormState, form: FormData) => Promise<FormState>;

function Refused({ state }: { state: FormState }) {
  if (!state.error) return null;
  return (
    <p role="alert" className="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-sm">
      {state.error}
    </p>
  );
}

/** The comment thread a decision or issue is being made from, quoted. */
function Source({ id, quote }: { id?: string; quote?: string }) {
  if (!id) return null;
  return (
    <>
      <input type="hidden" name="sourceCommentId" value={id} />
      <p className="border-l-2 border-violet-400 pl-2 text-sm text-muted-foreground line-clamp-4">
        From a comment: {quote}
      </p>
    </>
  );
}

export function DecisionForm({
  action,
  sourceId,
  quote,
  title,
}: {
  action: Action;
  sourceId?: string;
  quote?: string;
  title?: string;
}) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form key={state.attempt} action={submit} className="space-y-3 rounded-lg border p-4">
      <h2 className="font-medium">Record a decision</h2>
      <Refused state={state} />
      <Source id={sourceId} quote={quote} />
      <div className="space-y-1">
        <Label htmlFor="decision-title">Title</Label>
        <Input id="decision-title" name="title" required defaultValue={state.values?.title ?? title} />
      </div>
      <div className="space-y-1">
        <Label htmlFor="decision-description">What was decided</Label>
        <Textarea id="decision-description" name="description" required rows={3} defaultValue={state.values?.content} />
      </div>
      <div className="space-y-1">
        <Label htmlFor="decision-rationale">Why (optional)</Label>
        <Textarea id="decision-rationale" name="rationale" rows={2} defaultValue={state.values?.note} />
      </div>
      <Button type="submit" disabled={pending}>
        {pending ? "Recording…" : "Record decision"}
      </Button>
    </form>
  );
}

export function IssueForm({
  action,
  sourceId,
  quote,
  title,
  people,
}: {
  action: Action;
  sourceId?: string;
  quote?: string;
  title?: string;
  people: string[];
}) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form key={state.attempt} action={submit} className="space-y-3 rounded-lg border p-4">
      <h2 className="font-medium">Open an issue</h2>
      <Refused state={state} />
      <Source id={sourceId} quote={quote} />
      <div className="space-y-1">
        <Label htmlFor="issue-title">Title</Label>
        <Input id="issue-title" name="title" required defaultValue={state.values?.title ?? title} />
      </div>
      <div className="space-y-1">
        <Label htmlFor="issue-description">Details (optional)</Label>
        <Textarea id="issue-description" name="description" rows={3} defaultValue={state.values?.content} />
      </div>
      <div className="space-y-1">
        <Label htmlFor="issue-assignee">Assigned to (optional)</Label>
        <Input
          id="issue-assignee"
          name="assignedTo"
          list="bcs-tracking-people"
          defaultValue={state.values?.shepherds}
          placeholder="GitHub login"
        />
        <datalist id="bcs-tracking-people">
          {people.map((login) => (
            <option key={login} value={login} />
          ))}
        </datalist>
      </div>
      <Button type="submit" disabled={pending}>
        {pending ? "Opening…" : "Open issue"}
      </Button>
    </form>
  );
}

/** Resolving (with a resolution), reopening, or reassigning one issue. */
export function IssueActions({
  action,
  issueId,
  resolved,
  assignedTo,
}: {
  action: Action;
  issueId: string;
  resolved: boolean;
  assignedTo?: string;
}) {
  const [state, submit, pending] = useActionState(action, {});
  const [mode, setMode] = useState<"none" | "resolve" | "assign">("none");
  // Back to the buttons once a change is saved, so the issue's new state shows its actions.
  useEffect(() => {
    if (state.attempt && !state.error) setMode("none");
  }, [state.attempt, state.error]);
  return (
    <div className="space-y-2">
      <Refused state={state} />
      {mode === "resolve" && (
        <form key={state.attempt} action={submit} className="space-y-2">
          <input type="hidden" name="issueId" value={issueId} />
          <input type="hidden" name="change" value="resolve" />
          <Textarea
            name="resolution"
            required
            autoFocus
            rows={2}
            aria-label="Resolution"
            placeholder="How it was settled"
            defaultValue={state.values?.content}
          />
          <div className="flex gap-2">
            <Button type="submit" size="sm" disabled={pending}>
              Resolve
            </Button>
            <Button type="button" size="sm" variant="ghost" onClick={() => setMode("none")}>
              Cancel
            </Button>
          </div>
        </form>
      )}
      {mode === "assign" && (
        <form key={state.attempt} action={submit} className="flex gap-2">
          <input type="hidden" name="issueId" value={issueId} />
          <input type="hidden" name="change" value="assign" />
          <Input
            name="assignedTo"
            autoFocus
            defaultValue={assignedTo}
            list="bcs-tracking-people"
            aria-label="Assigned to"
            placeholder="GitHub login, or empty"
            className="h-8"
          />
          <Button type="submit" size="sm" disabled={pending}>
            Save
          </Button>
          <Button type="button" size="sm" variant="ghost" onClick={() => setMode("none")}>
            Cancel
          </Button>
        </form>
      )}
      {mode === "none" && (
        <div className="flex flex-wrap gap-2">
          {resolved ? (
            <form action={submit}>
              <input type="hidden" name="issueId" value={issueId} />
              <input type="hidden" name="change" value="reopen" />
              <Button type="submit" size="sm" variant="outline" disabled={pending}>
                Reopen
              </Button>
            </form>
          ) : (
            <Button type="button" size="sm" variant="outline" onClick={() => setMode("resolve")}>
              Resolve…
            </Button>
          )}
          <Button type="button" size="sm" variant="ghost" onClick={() => setMode("assign")}>
            {assignedTo ? "Reassign" : "Assign"}
          </Button>
        </div>
      )}
    </div>
  );
}

/** Deleting a decision or issue recorded by mistake, after confirming. */
export function RemoveButton({ action, name, id, what }: { action: Action; name: string; id: string; what: string }) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form
      action={submit}
      onSubmit={(event) => {
        if (!confirm(`Delete this ${what}?`)) event.preventDefault();
      }}
    >
      <input type="hidden" name={name} value={id} />
      <button type="submit" disabled={pending} className="text-xs text-muted-foreground hover:text-destructive">
        Delete
      </button>
      {state.error && <span className="ml-2 text-xs text-destructive">{state.error}</span>}
    </form>
  );
}
