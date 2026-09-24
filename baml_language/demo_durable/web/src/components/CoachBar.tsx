import type { Coach, ScenarioSession } from "../scenarios/useCoach";
import { AutoplaySwitch } from "./ScenarioGallery";

interface Props {
  session: ScenarioSession;
  coach: Coach;
  autoplay: boolean;
  live: boolean;
  onAutoplay(on: boolean): void;
  onStartRole(): void;
  onRestart(): void;
  onExit(): void;
  onOpenGallery(): void;
}

const STEP_GLYPHS = { done: "●", skipped: "◌", current: "◉", todo: "○" } as const;

/** The guide of the active scenario: its steps, the hint of the current step, and the autoplay switch. */
export function CoachBar({ session, coach, autoplay, live, onAutoplay, onStartRole, onRestart, onExit, onOpenGallery }: Props) {
  const { scenario } = session;
  const { progress } = coach;
  const step = scenario.steps[progress.index];
  const waiting = progress.action !== null && !progress.ready;
  return (
    <div className="coach" data-testid="coach" data-scenario={scenario.id} data-finished={progress.finished} data-step={step?.id ?? "done"}
      data-ready={progress.ready} role="status" aria-live="polite">
      <button className="coach-title" onClick={onOpenGallery} title="Open the scenario gallery">
        <span className="muted">Scenario</span> <b>{scenario.title}</b>
      </button>
      <ol className="coach-steps" aria-label="Steps">
        {scenario.steps.map((entry, index) => (
          <li key={entry.id} data-state={progress.states[index]} data-act={entry.action !== undefined} title={`${index + 1}. ${entry.title}${progress.states[index] === "skipped" ? " (skipped)" : ""}`}>
            <span aria-hidden="true">{STEP_GLYPHS[progress.states[index] ?? "todo"]}</span>
            <span className="coach-step-title">{entry.title}</span>
          </li>
        ))}
      </ol>
      <div className="coach-hint" data-testid="coach-hint" data-kind={progress.finished ? "done" : progress.action === null || waiting ? "watch" : "act"}>
        {!progress.finished && progress.action !== null && !waiting && <span className="coach-tag">{autoplay ? (live ? "autoplay" : "recorded") : progress.optional ? "optional" : "your turn"}</span>}
        {progress.hint}
      </div>
      {coach.start && (
        <button className="btn" data-testid="coach-start" data-coach={coach.start.mode} onClick={onStartRole}>
          Start {coach.start.action.start.fn}
        </button>
      )}
      <AutoplaySwitch on={autoplay} onChange={onAutoplay} />
      <button className="btn small" onClick={onRestart} title={live ? "Start the scenario again with a new run" : "Play the recording again"}>{live ? "Restart" : "Replay"}</button>
      <button className="btn small" onClick={onExit} title="Close the guide. The runs stay.">Exit</button>
    </div>
  );
}
