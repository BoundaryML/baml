"use client";

// Where the team stands on the version being read, as BEP stances: a summary in the header, and
// a panel listing each member's stance with the buttons to take, change or withdraw one's own.
import { useActionState } from "react";
import { formatDistanceToNow } from "date-fns";
import { Minus, ThumbsDown, ThumbsUp } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import type { FormState } from "../actions";

export type Stance = {
  author: string;
  stance: "support" | "neutral" | "oppose";
  comment?: string;
  updatedAt: number;
};

/** The same icons and colors as BEP stances. */
const LABELS = {
  support: { Icon: ThumbsUp, label: "Support", color: "text-green-600" },
  neutral: { Icon: Minus, label: "Neutral", color: "text-gray-500" },
  oppose: { Icon: ThumbsDown, label: "Oppose", color: "text-red-600" },
} as const;

function StanceIcon({ stance }: { stance: keyof typeof LABELS }) {
  const { Icon, color } = LABELS[stance];
  return <Icon className={`inline h-3.5 w-3.5 ${color}`} aria-hidden />;
}

export function StancePanel({
  action,
  version,
  stances,
  summary,
  viewer,
  error,
}: {
  /** Why stances could not be loaded: the panel says so instead of showing none. */
  error?: string;
  action: (previous: FormState, form: FormData) => Promise<FormState>;
  version: number;
  stances: Stance[];
  summary: { support: number; neutral: number; oppose: number };
  viewer: string;
}) {
  const [state, submit, pending] = useActionState(action, {});
  const mine = stances.find((each) => each.author === viewer);
  if (error) {
    return (
      <span className="text-sm rounded-md border px-3 py-1.5 text-muted-foreground" title={error}>
        Stances unavailable
      </span>
    );
  }
  return (
    <details className="relative">
      <summary
        className="cursor-pointer list-none text-sm rounded-md border px-3 py-1.5 hover:bg-muted"
        title={`Stances on v${version}`}
        aria-label={`Stances on v${version}: ${summary.support} support, ${summary.neutral} neutral, ${summary.oppose} oppose`}
      >
        <span className="inline-flex items-center gap-2" aria-hidden>
          <span className="inline-flex items-center gap-1"><StanceIcon stance="support" /> {summary.support}</span>
          <span className="inline-flex items-center gap-1"><StanceIcon stance="neutral" /> {summary.neutral}</span>
          <span className="inline-flex items-center gap-1"><StanceIcon stance="oppose" /> {summary.oppose}</span>
        </span>
      </summary>
      <div className="absolute right-0 z-10 mt-1 w-80 rounded-md border bg-background p-3 shadow-lg space-y-3">
        <h2 className="text-sm font-medium">Stances on v{version}</h2>
        {stances.length === 0 ? (
          <p className="text-sm text-muted-foreground">Nobody has taken a stance on this version yet.</p>
        ) : (
          <ul className="space-y-2 max-h-64 overflow-y-auto">
            {stances.map((each) => (
              <li key={each.author} className="text-sm">
                <StanceIcon stance={each.stance} />{" "}
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
                <StanceIcon stance={key} /> {LABELS[key].label}
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
