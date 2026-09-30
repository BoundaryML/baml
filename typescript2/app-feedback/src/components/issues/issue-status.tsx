import { Badge } from '@/components/ui/badge';
import { statusLabel } from '@/lib/pipeline';
import type { Difficulty, Issue, Subsystem } from '@/lib/types';

/** Leads every issue view: a bug, or a feature request. */
export function KindBadge({
  kind,
  className,
}: {
  kind: Issue['kind'];
  className?: string;
}) {
  return kind === 'feature' ? (
    <Badge
      className={
        'border-violet-300 bg-violet-50 text-violet-800 dark:border-violet-700 dark:bg-violet-950/50 dark:text-violet-200 ' +
        (className ?? '')
      }
      variant="outline"
    >
      Feature request
    </Badge>
  ) : (
    <Badge
      className={
        'border-slate-300 text-slate-700 dark:border-slate-600 dark:text-slate-300 ' +
        (className ?? '')
      }
      variant="outline"
    >
      Bug
    </Badge>
  );
}

export function StatusBadge({ issue }: { issue: Issue }) {
  return <Badge variant={issue.status.state}>{statusLabel(issue)}</Badge>;
}

const DIFFICULTY_VARIANT: Record<
  Difficulty,
  'trivial' | 'easy' | 'medium' | 'hard'
> = {
  Easy: 'easy',
  Hard: 'hard',
  Medium: 'medium',
  Trivial: 'trivial',
};

export function DifficultyBadge({
  difficulty,
}: {
  difficulty: Difficulty | null;
}) {
  if (!difficulty) {
    return (
      <Badge className="text-muted-foreground font-normal" variant="outline">
        not gauged
      </Badge>
    );
  }
  return <Badge variant={DIFFICULTY_VARIANT[difficulty]}>{difficulty}</Badge>;
}

const SUBSYSTEM_LABEL: Record<Subsystem, string> = {
  Compiler: 'compiler',
  Runtime: 'runtime',
  StdLibrary: 'stdlib',
  Syntax: 'syntax',
  Tooling: 'tooling',
  Unknown: 'unknown',
};

export function SubsystemBadge({ subsystem }: { subsystem: Subsystem }) {
  return (
    <Badge className="font-mono" variant="subsystem">
      {SUBSYSTEM_LABEL[subsystem]}
    </Badge>
  );
}
