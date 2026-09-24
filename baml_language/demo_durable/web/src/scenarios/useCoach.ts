/**
 * The scenario guide at run time: it follows the progress of the active
 * scenario, selects the run that the current step acts on, points at the
 * control of that step, and, with autoplay, presses it.
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { clock } from "../clock";
import type { CoachTarget } from "../components/RunsPanel";
import type { AppState, RunKey } from "../state";
import { scenarioProgress, scenarioView, type RoleRefs, type RunStart, type Scenario, type ScenarioProgress, type StepAction } from "./engine";

/** The time between the moment a step becomes possible and the moment autoplay presses its control. The highlight is visible in between. */
export const AUTOPLAY_PRESS_DELAY_MS = 1300;

export interface ScenarioSession {
  scenario: Scenario;
  roles: RoleRefs;
}

export interface Coach {
  progress: ScenarioProgress;
  /** The control of the runs panel that the guide points at. */
  target: CoachTarget | null;
  /** The `start` action of the current step, which the guide bar offers as a button. */
  start: { action: Extract<StepAction, { kind: "start" }>; mode: "hint" | "press" } | null;
}

interface Options {
  session: ScenarioSession | null;
  state: Pick<AppState, "runs" | "events" | "selected">;
  autoplay: boolean;
  /** Commands reach a site server. In fixture mode the guide only points. */
  live: boolean;
  select(key: RunKey): void;
  startRole(role: "plain", start: RunStart): void;
}

export function useCoach({ session, state, autoplay, live, select, startRole }: Options): Coach | null {
  // The hints count down and name moving numbers, so the view follows the clock while a scenario runs.
  const [now, setNow] = useState(() => clock.now());
  const active = session !== null;
  useEffect(() => {
    if (!active) return;
    const id = setInterval(() => setNow(clock.now()), 250);
    return () => clearInterval(id);
  }, [active]);

  const progress = useMemo(
    () => (session ? scenarioProgress(session.scenario, scenarioView(state, session.roles, now)) : null),
    [session, state.runs, state.events, now], // eslint-disable-line react-hooks/exhaustive-deps
  );

  const action = progress?.action ?? null;
  const ready = progress?.ready ?? false;
  const stepKey = session && action ? `${session.scenario.id}:${action.step}:${action.target ?? ""}` : null;

  // Select the run that the step acts on. Without autoplay this happens once
  // per step, and after that the user's selection wins. With autoplay the guide
  // drives the controls, so it keeps its target selected until it has pressed.
  const selectedFor = useRef<string | null>(null);
  const pressedFor = useRef<string | null>(null);
  useEffect(() => {
    if (stepKey === null || !ready || action === null || action.action.kind !== "command" || action.target === null) return;
    const driving = autoplay && live && pressedFor.current !== stepKey;
    if (selectedFor.current === stepKey && !driving) return;
    selectedFor.current = stepKey;
    if (state.selected !== action.target) select(action.target);
  }, [stepKey, ready, action, state.selected, select, autoplay, live]);

  // Autoplay: show the highlight for a moment, then press.
  const [press, setPress] = useState<{ key: string; nonce: number } | null>(null);
  useEffect(() => {
    if (!autoplay || !live || stepKey === null || !ready || action === null) return;
    if (pressedFor.current === stepKey) return;
    const timer = setTimeout(() => {
      pressedFor.current = stepKey;
      if (action.action.kind === "start") startRole(action.action.as, action.action.start);
      else setPress((current) => ({ key: stepKey, nonce: (current?.nonce ?? 0) + 1 }));
    }, AUTOPLAY_PRESS_DELAY_MS);
    return () => clearTimeout(timer);
    // `action` changes identity with every progress. The step key names it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoplay, live, stepKey, ready, startRole]);

  if (!session || !progress) return null;
  const mode = autoplay ? "press" : "hint";
  const target: CoachTarget | null =
    action && ready && action.action.kind === "command" && action.target !== null
      ? {
          run: action.target,
          action: action.action.action,
          ...(action.action.site === undefined ? {} : { site: action.action.site }),
          mode,
          nonce: live && press !== null && press.key === stepKey ? press.nonce : null,
        }
      : null;
  const start = action && ready && action.action.kind === "start" ? { action: action.action, mode } as const : null;
  return { progress, target, start };
}
