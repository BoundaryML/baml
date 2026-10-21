import { MOCK_SCENARIOS } from '../sources/mock';

interface ScenarioBarProps {
  /** The scenario being replayed, or `null` when connected to a live server. */
  scenario: string | null;
  onScenario: (id: string | null) => void;
  onRestart: () => void;
}

/** Pick a recorded session to replay, or switch back to the live server. */
export function ScenarioBar({
  scenario,
  onScenario,
  onRestart,
}: ScenarioBarProps) {
  return (
    <div aria-label="Session source" className="aa-scenarios" role="toolbar">
      <span className="aa-scenarios-label">Replay a recorded session</span>
      <div className="aa-scenario-buttons">
        {MOCK_SCENARIOS.map((s) => (
          <button
            aria-pressed={scenario === s.id}
            className="aa-scenario"
            key={s.id}
            onClick={() => (scenario === s.id ? onRestart() : onScenario(s.id))}
            title={s.description}
            type="button"
          >
            <span className="aa-scenario-title">{s.title}</span>
            <span className="aa-scenario-meta">{s.summary}</span>
          </button>
        ))}
      </div>
      <div className="aa-scenarios-end">
        {scenario !== null && (
          <button className="aa-text-button" onClick={onRestart} type="button">
            Restart
          </button>
        )}
        <button
          aria-pressed={scenario === null}
          className="aa-text-button"
          onClick={() => onScenario(null)}
          title="Stream edits from `baml agent watch` on this address"
          type="button"
        >
          Live
        </button>
      </div>
    </div>
  );
}
