"use client";

// The BCS forms. They submit to the server actions in actions.ts and, when a save is refused,
// show why above the fields and keep what the editor submitted in them.
import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { BCS_STATUSES } from "@/lib/bcs-convex";
import type { FormState } from "./actions";
import { HtmlField } from "./proposal-fields";

type Action = (previous: FormState, form: FormData) => Promise<FormState>;

function Refused({ state }: { state: FormState }) {
  if (!state.error) return null;
  return (
    <p role="alert" className="rounded-md border border-destructive/40 bg-destructive/10 p-2 text-sm">
      {state.error}
    </p>
  );
}

function StatusSelect({ id, value }: { id?: string; value: string }) {
  return (
    <select
      id={id}
      name="status"
      defaultValue={value}
      aria-label={id ? undefined : "Status"}
      className="h-9 rounded-md border bg-background px-3 text-sm"
    >
      {BCS_STATUSES.map((status) => (
        <option key={status} value={status}>
          {status}
        </option>
      ))}
    </select>
  );
}

export function NewProposalForm({ action }: { action: Action }) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form key={state.attempt} action={submit} className="space-y-5">
      <Refused state={state} />
      <div className="space-y-2">
        <Label htmlFor="title">Title</Label>
        <Input
          id="title"
          name="title"
          required
          defaultValue={state.values?.title}
          placeholder="Alerts on Alien Commands"
        />
      </div>
      <div className="space-y-2">
        <Label htmlFor="status">Status</Label>
        <StatusSelect id="status" value={state.values?.status ?? "proposed"} />
      </div>
      <HtmlField defaultValue={state.values?.html} />
      <Button type="submit" disabled={pending}>
        {pending ? "Creating…" : "Create"}
      </Button>
    </form>
  );
}

export function SaveVersionForm({ action, baseVersion }: { action: Action; baseVersion: number }) {
  const [state, submit, pending] = useActionState(action, {});
  const base = state.baseVersion ?? baseVersion;
  return (
    <form key={state.attempt} action={submit} className="space-y-3">
      <h2 className="font-medium">Save a new version</h2>
      <Refused state={state} />
      <input type="hidden" name="baseVersion" value={base} />
      <HtmlField defaultValue={state.values?.html} />
      <div className="space-y-2">
        <Label htmlFor="note">What changed</Label>
        <Input
          id="note"
          name="note"
          defaultValue={state.values?.note}
          placeholder="Answered Paulo's questions"
        />
      </div>
      <Button type="submit" size="sm" disabled={pending}>
        {pending ? "Saving…" : `Save v${base + 1}`}
      </Button>
    </form>
  );
}

export function UpdateProposalForm({
  action,
  title,
  status,
}: {
  action: Action;
  title: string;
  status: string;
}) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form key={state.attempt} action={submit} className="space-y-3 border-t pt-4">
      <h2 className="font-medium">Title and status</h2>
      <Refused state={state} />
      <input type="hidden" name="originalTitle" value={title} />
      <input type="hidden" name="originalStatus" value={status} />
      <Input name="title" defaultValue={state.values?.title ?? title} aria-label="Title" />
      <StatusSelect value={state.values?.status ?? status} />
      <Button type="submit" size="sm" variant="outline" disabled={pending}>
        Update
      </Button>
    </form>
  );
}
