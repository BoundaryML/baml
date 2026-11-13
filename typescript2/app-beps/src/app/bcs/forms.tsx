"use client";

// The BCS forms. They submit to the server actions in actions.ts and, when a save is refused,
// show why above the fields and keep what the editor submitted in them.
import { useActionState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { BCS_STATUSES } from "@/lib/bcs-convex";
import type { FormState } from "./actions";
import { ContentField } from "./proposal-fields";

type Action = (previous: FormState, form: FormData) => Promise<FormState>;

/** A page that can be another page's parent: its slug, title and depth in the outline. */
export type ParentChoice = { slug: string; title: string; depth: number };

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

function ParentSelect({ choices, value }: { choices: ParentChoice[]; value?: string }) {
  return (
    <div className="space-y-2">
      <Label htmlFor="parentSlug">Inside</Label>
      <select
        id="parentSlug"
        name="parentSlug"
        defaultValue={value ?? ""}
        className="h-9 rounded-md border bg-background px-3 text-sm"
      >
        <option value="">The main page (top level)</option>
        {choices.map((choice) => (
          <option key={choice.slug} value={choice.slug}>
            {"  ".repeat(choice.depth)}
            {choice.title}
          </option>
        ))}
      </select>
    </div>
  );
}

function Note({ value }: { value?: string }) {
  return (
    <div className="space-y-2">
      <Label htmlFor="note">What changed</Label>
      <Input id="note" name="note" defaultValue={value} placeholder="Answered Paulo's questions" />
    </div>
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
      <ContentField defaultValue={state.values?.content} defaultFormat={state.values?.format} />
      <p className="text-sm text-muted-foreground">
        This is the main page. Add sub-pages once the proposal exists.
      </p>
      <Button type="submit" disabled={pending}>
        {pending ? "Creating…" : "Create"}
      </Button>
    </form>
  );
}

/** Edits one page: its content, format, and for a sub-page, where it sits. */
export function EditPageForm({
  action,
  baseVersion,
  content,
  format,
  parentSlug,
  parents,
}: {
  action: Action;
  baseVersion: number;
  content: string;
  format: string;
  // Absent for the main page, which has no parent.
  parentSlug?: string;
  parents?: ParentChoice[];
}) {
  const [state, submit, pending] = useActionState(action, {});
  const base = state.baseVersion ?? baseVersion;
  return (
    <form key={state.attempt} action={submit} className="space-y-5">
      <Refused state={state} />
      <input type="hidden" name="baseVersion" value={base} />
      {parents && <ParentSelect choices={parents} value={state.values?.parentSlug ?? parentSlug} />}
      <ContentField
        defaultValue={state.values?.content ?? content}
        defaultFormat={state.values?.format ?? format}
        rows={28}
      />
      <Note value={state.values?.note} />
      <Button type="submit" disabled={pending}>
        {pending ? "Saving…" : `Save v${base + 1}`}
      </Button>
    </form>
  );
}

export function AddPageForm({
  action,
  baseVersion,
  parents,
  parentSlug,
}: {
  action: Action;
  baseVersion: number;
  parents: ParentChoice[];
  parentSlug?: string;
}) {
  const [state, submit, pending] = useActionState(action, {});
  const base = state.baseVersion ?? baseVersion;
  return (
    <form key={state.attempt} action={submit} className="space-y-5">
      <Refused state={state} />
      <input type="hidden" name="baseVersion" value={base} />
      <div className="space-y-2">
        <Label htmlFor="slug">Slug</Label>
        <Input
          id="slug"
          name="slug"
          required
          pattern="[a-z0-9][a-z0-9-]*"
          defaultValue={state.values?.slug}
          placeholder="scheduler"
        />
        <p className="text-sm text-muted-foreground">
          Lowercase letters, digits and dashes: the page&apos;s address, and its file name with
          <code> bep bcs</code>. Its title is its first # heading.
        </p>
      </div>
      <ParentSelect choices={parents} value={state.values?.parentSlug ?? parentSlug} />
      <ContentField defaultValue={state.values?.content} defaultFormat={state.values?.format} rows={20} />
      <Note value={state.values?.note} />
      <Button type="submit" disabled={pending}>
        {pending ? "Adding…" : `Add it as v${base + 1}`}
      </Button>
    </form>
  );
}

export function RemovePageForm({
  action,
  baseVersion,
  title,
}: {
  action: Action;
  baseVersion: number;
  title: string;
}) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form
      key={state.attempt}
      action={submit}
      onSubmit={(event) => {
        if (!confirm(`Remove "${title}"? Earlier versions keep it.`)) event.preventDefault();
      }}
      className="space-y-2 border-t pt-4"
    >
      <h2 className="font-medium">Remove this page</h2>
      <Refused state={state} />
      <input type="hidden" name="baseVersion" value={state.baseVersion ?? baseVersion} />
      <Button type="submit" size="sm" variant="outline" disabled={pending}>
        Remove &ldquo;{title}&rdquo;
      </Button>
    </form>
  );
}

export function UpdateProposalForm({ action, title }: { action: Action; title: string }) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form key={state.attempt} action={submit} className="space-y-3">
      <h2 className="font-medium">Title</h2>
      <Refused state={state} />
      <input type="hidden" name="originalTitle" value={title} />
      <Input name="title" defaultValue={state.values?.title ?? title} aria-label="Title" />
      <Button type="submit" size="sm" variant="outline" disabled={pending}>
        Rename
      </Button>
    </form>
  );
}

/** The header's status menu: choosing a status saves it. */
export function HeaderStatus({ action, status }: { action: Action; status: string }) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form key={state.attempt} action={submit} className="flex items-center gap-2">
      <select
        name="status"
        defaultValue={status}
        aria-label="Status"
        disabled={pending}
        onChange={(event) => event.currentTarget.form?.requestSubmit()}
        className="h-8 rounded-md border bg-background px-2 text-sm capitalize"
      >
        {BCS_STATUSES.map((each) => (
          <option key={each} value={each}>
            {each}
          </option>
        ))}
      </select>
      {state.error && (
        <span role="alert" className="text-xs text-destructive">
          {state.error}
        </span>
      )}
    </form>
  );
}

/** Who shepherds the proposal, and once implemented, who implemented it. */
export function PeopleForm({
  action,
  shepherds,
  implementedBy,
  showImplemented,
  people,
}: {
  action: Action;
  shepherds: string[];
  implementedBy: string[];
  showImplemented: boolean;
  people: string[];
}) {
  const [state, submit, pending] = useActionState(action, {});
  return (
    <form key={state.attempt} action={submit} className="space-y-3">
      <h2 className="font-medium">People</h2>
      <Refused state={state} />
      <div className="space-y-1">
        <Label htmlFor="shepherds">Shepherds</Label>
        <Input
          id="shepherds"
          name="shepherds"
          list="bcs-people"
          defaultValue={state.values?.shepherds ?? shepherds.join(", ")}
          placeholder="GitHub logins, comma-separated"
        />
      </div>
      {showImplemented && (
        <div className="space-y-1">
          <Label htmlFor="implementedBy">Implemented by</Label>
          <Input
            id="implementedBy"
            name="implementedBy"
            list="bcs-people"
            defaultValue={state.values?.implementedBy ?? implementedBy.join(", ")}
            placeholder="GitHub logins, comma-separated"
          />
        </div>
      )}
      <datalist id="bcs-people">
        {people.map((login) => (
          <option key={login} value={login} />
        ))}
      </datalist>
      <Button type="submit" size="sm" variant="outline" disabled={pending}>
        Save
      </Button>
    </form>
  );
}
