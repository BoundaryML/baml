import { useEffect, useState } from "react";
import type { Scenario } from "../scenarios/engine";
import { SCENARIOS } from "../scenarios/catalog";
import { MiniTimeline } from "./MiniTimeline";

interface Props {
  /** Live mode: "Run" starts the scenario on the site servers. Fixture mode: only the recordings play. */
  live: boolean;
  /** Whether the registry site answered. Without it a scenario cannot start. */
  connected: boolean;
  active: Scenario["id"] | null;
  autoplay: boolean;
  onAutoplay(on: boolean): void;
  onRun(scenario: Scenario): Promise<void>;
  onClose(): void;
}

/** The link that plays the recording of a scenario without servers. */
export function recordingHref(scenario: Scenario, autoplay: boolean): string {
  return `?fixture=${scenario.fixture}&speed=2${autoplay ? "&autoplay=1" : ""}`;
}

/** The scenario gallery: a sheet over the app with one card per scenario (contract section 9.6). */
export function ScenarioGallery({ live, connected, active, autoplay, onAutoplay, onRun, onClose }: Props) {
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<{ id: string; message: string } | null>(null);

  useEffect(() => {
    const onKey = (event: KeyboardEvent): void => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);

  const run = async (scenario: Scenario): Promise<void> => {
    setBusy(scenario.id);
    setError(null);
    try {
      await onRun(scenario);
    } catch (cause) {
      setError({ id: scenario.id, message: cause instanceof Error ? cause.message : String(cause) });
    } finally {
      setBusy(null);
    }
  };

  return (
    <div className="sheet-backdrop" data-testid="scenario-gallery" onClick={onClose}>
      <div className="sheet" role="dialog" aria-modal="true" aria-label="Scenario gallery" onClick={(event) => event.stopPropagation()}>
        <div className="sheet-head">
          <div>
            <h2>Scenarios</h2>
            <p className="muted">
              Each scenario starts one function of the demo program and guides you through it. The hints follow the event stream, so they
              appear at the moment the action makes sense.
            </p>
          </div>
          <span className="spacer" />
          <AutoplaySwitch on={autoplay} onChange={onAutoplay} />
          <button className="btn" onClick={onClose} aria-label="Close the scenario gallery">Close</button>
        </div>
        <div className="sheet-grid">
          {SCENARIOS.map((scenario, index) => (
            <article key={scenario.id} className="scenario-card" data-testid="scenario-card" data-scenario={scenario.id} data-active={scenario.id === active}>
              <header>
                <span className="scenario-num">{index + 1}</span>
                <h3>{scenario.title}</h3>
              </header>
              <MiniTimeline fixture={scenario.fixture} />
              <p>{scenario.summary}</p>
              <div className="scenario-starts mono" title="The function and the arguments that the scenario starts">
                {scenario.start.fn}({JSON.stringify(scenario.start.args)}) <span className="muted">on {scenario.start.site}</span>
              </div>
              <ol className="scenario-steps">
                {scenario.steps.map((step) => (
                  <li key={step.id} data-act={step.action !== undefined} data-optional={step.optional ?? false}
                    title={step.action ? (step.optional ? "an optional action" : "an action of yours, or of autoplay") : "something to watch"}>
                    {step.title}
                  </li>
                ))}
              </ol>
              <div className="scenario-tags">
                {scenario.shows.map((tag) => <span key={tag}>{tag}</span>)}
              </div>
              <footer>
                <button className="btn primary" data-testid="scenario-run" disabled={!live || !connected || busy !== null}
                  title={live ? (connected ? "Start the function on the site servers and follow the guide" : "The local site server is not connected") : "This page plays a recording. Open the live app to run the scenario on the site servers."}
                  onClick={() => void run(scenario)}>
                  {busy === scenario.id ? "Starting…" : "Run live"}
                </button>
                <a className="btn" data-testid="scenario-play" href={recordingHref(scenario, autoplay)} title="Play a recording of the scenario. It needs no site servers.">
                  Play recording
                </a>
              </footer>
              {error?.id === scenario.id && <div className="error-line" role="alert">{error.message}</div>}
            </article>
          ))}
        </div>
      </div>
    </div>
  );
}

export function AutoplaySwitch({ on, onChange }: { on: boolean; onChange(on: boolean): void }) {
  return (
    <button type="button" className="switch" role="switch" aria-checked={on} data-testid="autoplay" onClick={() => onChange(!on)}
      title="Autoplay performs the hinted actions. It highlights the button for a moment and then presses it.">
      <span className="switch-track"><span className="switch-thumb" /></span>
      Autoplay
    </button>
  );
}
