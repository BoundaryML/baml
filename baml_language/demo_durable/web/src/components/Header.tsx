import { useEffect, useMemo, useState } from "react";
import { REGISTRY_SITE, type JsonObject, type Site, type SiteEntry, type SiteInfo } from "../protocol";
import { connectionOf, pickerFunctions, type AppState } from "../state";

const CUSTOM = "__custom__";
const DEFAULT_ARGS = '{"city":"Lisbon"}';

interface Props {
  /** The site registry, in registry order. */
  sites: readonly SiteEntry[];
  connections: AppState["connections"];
  fixture: { name: string; title: string; speed: number; onReplay(): void } | null;
  onStart(site: Site, fn: string, args: JsonObject): Promise<void>;
}

const CONNECTION_LABELS = {
  connecting: "connecting",
  open: "connected",
  disconnected: "disconnected, retrying",
  fixture: "fixture",
} as const;

function parseArgs(text: string): { args: JsonObject | null; error: string | null } {
  try {
    const value: unknown = JSON.parse(text);
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
      return { args: null, error: "args must be a JSON object keyed by parameter name" };
    }
    return { args: value as JsonObject, error: null };
  } catch (cause) {
    return { args: null, error: cause instanceof Error ? cause.message : "invalid JSON" };
  }
}

export function Header({ sites, connections, fixture, onStart }: Props) {
  // Every site runs the same program. The first site that answered names the functions.
  const info: SiteInfo | null = sites.map((entry) => connectionOf({ connections }, entry.name).info).find((candidate) => candidate !== null) ?? null;
  const functions = useMemo(() => pickerFunctions(info), [info]);
  const [choice, setChoice] = useState<string>(CUSTOM);
  const [custom, setCustom] = useState("durable_plan_trip");
  const [argsText, setArgsText] = useState(DEFAULT_ARGS);
  const [chosenSite, setSite] = useState<Site>(REGISTRY_SITE);
  // The chosen site can leave the registry. The first site takes its place.
  const site = sites.some((entry) => entry.name === chosenSite) ? chosenSite : (sites[0]?.name ?? chosenSite);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Once the function list arrives, prefer the durable demo function.
  useEffect(() => {
    if (functions.length === 0) {
      setChoice(CUSTOM);
      return;
    }
    setChoice((current) => {
      if (current !== CUSTOM && functions.some((fn) => fn.name === current)) return current;
      return (functions.find((fn) => fn.name === "durable_plan_trip") ?? functions[0])?.name ?? CUSTOM;
    });
  }, [functions]);

  const fnName = choice === CUSTOM ? custom.trim() : choice;
  const parsed = parseArgs(argsText);
  const selectedInfo = functions.find((fn) => fn.name === fnName);

  const start = async (): Promise<void> => {
    if (parsed.args === null || fnName === "") return;
    setBusy(true);
    setError(null);
    try {
      await onStart(site, fnName, parsed.args);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <header className="header" data-testid="header">
      <div className="brand">
        <b>BAML durable functions</b>
        <div className="conns">
          {sites.map(({ name, url }) => {
            const status = connectionOf({ connections }, name).status;
            return (
              <span key={name} className="conn" data-status={status} data-testid={`conn-${name}`}
                title={`${name}${url ? ` (${url})` : ""}: ${CONNECTION_LABELS[status]}`}>
                <i className="site-dot" data-site={name} />
                {name} · {CONNECTION_LABELS[status]}
              </span>
            );
          })}
        </div>
      </div>

      <form className="start-form" onSubmit={(event) => { event.preventDefault(); void start(); }}>
        <label>
          Function
          <span className="fn-controls">
            {functions.length > 0 && (
              <select className="input mono" value={choice} onChange={(event) => setChoice(event.target.value)} data-testid="fn-select">
                {functions.map((fn) => (
                  <option key={fn.name} value={fn.name}>
                    {fn.name}
                    {fn.durable ? "  [durable]" : ""}
                    {fn.remote ? "  [remote]" : ""}
                  </option>
                ))}
                <option value={CUSTOM}>other…</option>
              </select>
            )}
            {choice === CUSTOM && (
              <input className="input mono" value={custom} onChange={(event) => setCustom(event.target.value)}
                placeholder="function name" spellCheck={false} aria-label="Function name" data-testid="fn-text" style={{ width: 190 }} />
            )}
          </span>
        </label>
        <label className="args">
          <span>
            Args (JSON){selectedInfo && selectedInfo.params.length > 0 && (
              <span className="mono" style={{ textTransform: "none" }}>
                {" "}· {selectedInfo.params.map((param) => `${param.name}: ${param.type}`).join(", ")}
              </span>
            )}
          </span>
          <textarea className="input" value={argsText} onChange={(event) => setArgsText(event.target.value)}
            spellCheck={false} aria-invalid={parsed.error !== null} data-testid="args" />
        </label>
        <div className="go">
          <span className="seg" role="group" aria-label="Site to start on">
            {sites.map(({ name }) => (
              <button key={name} type="button" aria-pressed={site === name} onClick={() => setSite(name)}>
                <i className="site-dot" data-site={name} />
                {name}
              </button>
            ))}
          </span>
          <button className="btn primary" type="submit" disabled={busy || parsed.args === null || fnName === ""} data-testid="start">
            Start
          </button>
        </div>
      </form>

      {(parsed.error ?? error) && <div className="error-line" role="alert">{parsed.error ?? error}</div>}

      {fixture && (
        <div className="fixture-tag" data-testid="fixture-tag" title={fixture.title}>
          fixture <b>{fixture.name}</b>
          <span className="muted">{Number.isFinite(fixture.speed) ? `${fixture.speed}× speed` : "instant"}</span>
          <button className="btn small" onClick={fixture.onReplay}>Replay</button>
        </div>
      )}
    </header>
  );
}
