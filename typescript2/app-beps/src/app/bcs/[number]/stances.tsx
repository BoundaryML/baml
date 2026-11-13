"use client";

// Where the team stands on the version being read, as BEP stances: a summary in the header, and
// a panel listing each member's stance with the buttons to take, change or withdraw one's own.
import { useActionState } from "react";
import { formatDistanceToNow } from "date-fns";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { FormState } from "../actions";

export type Stance = {
  author: string;
  stance: "support" | "neutral" | "oppose";
  comment?: string;
  updatedAt: number;
};

const LABELS = {
  support: { icon: "👍", label: "Support" },
  neutral: { icon: "😐", label: "Neutral" },
  oppose: { icon: "👎", label: "Oppose" },
} as const;

export function StancePanel({
  action,
  version,
  stances,
  summary,
  viewer,
}: {
  action: (previous: FormState, form: FormData) => Promise<FormState>;
  version: number;
  stances: Stance[];
  summary: { support: number; neutral: number; oppose: number };
  viewer: string;
}) {
  const [state, submit, pending] = useActionState(action, {});
  const mine = stances.find((each) => each.author === viewer);
  return (
    <details className="relative">
      <summary
        className="cursor-pointer list-none text-sm rounded-md border px-3 py-1.5 hover:bg-muted"
        title={`Stances on v${version}`}
      >
        {LABELS.support.icon} {summary.support} · {LABELS.neutral.icon} {summary.neutral} ·{" "}
        {LABELS.oppose.icon} {summary.oppose}
      </summary>
      <div className="absolute right-0 z-10 mt-1 w-80 rounded-md border bg-background p-3 shadow-lg space-y-3">
        <h2 className="text-sm font-medium">Stances on v{version}</h2>
        {stances.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nobody has taken a stance on this version yet.</p>
        ) : (
          <ul className="space-y-2 max-h-64 overflow-y-auto">
            {stances.map((each) => (
              <li key={each.author} className="text-sm">
                <span>{LABELS[each.stance].icon}</span>{" "}
                <span className="font-medium">{each.author}</span>{" "}
                <span className="text-xs text-muted-foreground">
                  {LABELS[each.stance].label.toLowerCase()} ·{" "}
                  {formatDistanceToNow(each.updatedAt, { addSuffix: true })}
                </span>
                {each.comment && <p className="text-xs text-muted-foreground">{each.comment}</p>}
              </li>
            ))}
          </ul>
        )}
        <form key={state.attempt} action={submit} className="space-y-2 border-t pt-3">
          <p className="text-xs text-muted-foreground">
            {mine ? `Yours: ${LABELS[mine.stance].label.toLowerCase()}.` : "Where do you stand?"}
          </p>
          {state.error && (
            <p role="alert" className="text-xs text-destructive">
              {state.error}
            </p>
          )}
          <Input
            name="comment"
            defaultValue={state.values?.note ?? mine?.comment ?? ""}
            placeholder="Why (optional)"
            aria-label="Why"
            className="h-8 text-sm"
          />
          <div className="flex flex-wrap gap-1">
            {(Object.keys(LABELS) as (keyof typeof LABELS)[]).map((key) => (
              <Button
                key={key}
                type="submit"
                name="stance"
                value={key}
                size="sm"
                variant={mine?.stance === key ? "default" : "outline"}
                disabled={pending}
              >
                {LABELS[key].icon} {LABELS[key].label}
              </Button>
            ))}
            {mine && (
              <Button type="submit" name="stance" value="clear" size="sm" variant="ghost" disabled={pending}>
                Withdraw
              </Button>
            )}
          </div>
        </form>
      </div>
    </details>
  );
}
