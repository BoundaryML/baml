import { useCallback, useEffect, useState } from 'react';
import { formatSource, loadFormatter } from './format';
import { Home } from './home';
import {
  Claims,
  Code,
  CompilerSays,
  PointsRule,
  signed,
  Teach,
} from './panels';
import {
  type Answered,
  answerCase,
  freshProfile,
  given,
  isSaid,
  type Knobs,
  knobsFrom,
  knobValues,
  type Next,
  nextPrompt,
  type Profile,
  pointsOf,
  replay,
  type Said,
  type Standing,
  standingOf,
  type Taken,
  taken,
  transcriptJson,
  Why,
} from './quiz';
import { Readout } from './readout';
import {
  clearSlot,
  readSlots,
  type Save,
  type SavedAnswer,
  type Slot,
  VERSION,
  writeSlot,
} from './saves';
import { type Chosen, Setup } from './setup';

/** A sitting in progress: what the page keeps, and what follows from it. */
interface Live {
  slot: number;
  session: number;
  full: boolean;
  knobs: Knobs;
  history: Taken[];
  /**
   * Always what `replay(knobs, history)` gives. It is carried rather than
   * worked out again on every answer, and every change to `history` below
   * carries the profile the engine returned with it; loading a save is the
   * one place it is worked out from scratch.
   */
  profile: Profile;
}

/** A file of a case, laid out by the formatter. */
interface Shown {
  name: string;
  source: string;
}

/** Where the page has got to. */
type Phase =
  | { kind: 'home' }
  | { kind: 'setup'; slot: number }
  | { kind: 'asking'; live: Live; next: Next; shown: Shown[] }
  | {
      kind: 'revealed';
      live: Live;
      next: Next;
      shown: Shown[];
      said: Said;
      answered: Answered;
    }
  | { kind: 'done'; live: Live; standing: Standing; starved: boolean }
  | { kind: 'broken'; message: string };

type Posed = Extract<Phase, { kind: 'asking' | 'revealed' }>;

const storage = window.localStorage;

function savedAnswer(kept: Taken): SavedAnswer {
  const { said } = kept.given;
  if (!isSaid(said)) {
    // Every answer the page keeps was built from a `Said`, so this cannot
    // happen; it is checked rather than cast.
    throw new Error(`an answer the page never offered: ${said}`);
  }
  return {
    item: kept.item,
    mark: kept.given.mark,
    reasoning: kept.given.reasoning,
    said,
    seed: kept.seed.toString(),
  };
}

function saveOf(live: Live): Save {
  return {
    answers: live.history.map(savedAnswer),
    full: live.full,
    knobs: knobValues(live.knobs),
    session: live.session,
    updated: new Date().toISOString(),
    version: VERSION,
  };
}

/** What a save holds, as the engine takes it. */
function heldBy(save: Save): { knobs: Knobs; history: Taken[] } {
  return {
    history: save.answers.map((a) =>
      taken(a.item, BigInt(a.seed), given(a.said, a.reasoning, a.mark)),
    ),
    knobs: knobsFrom(save.knobs),
  };
}

function fromSave(slot: number, save: Save): Live {
  const { knobs, history } = heldBy(save);
  return {
    full: save.full,
    history,
    knobs,
    profile: replay(knobs, history),
    session: save.session,
    slot,
  };
}

/** The phase a sitting is in given its answers so far: the next case, or the end. */
function advance(live: Live): Phase {
  const standing = standingOf(live.profile, live.knobs);
  if (standing.why !== Why.Continue) {
    return { kind: 'done', live, standing, starved: false };
  }
  const next = nextPrompt(
    live.profile,
    live.knobs,
    live.session,
    live.history.length,
  );
  if (next === null) {
    return { kind: 'done', live, standing, starved: true };
  }
  const shown = Object.entries(next.prompt.files).map(([name, source]) => ({
    name,
    source: formatSource(source),
  }));
  return { kind: 'asking', live, next, shown };
}

function download(name: string, json: string): void {
  const url = URL.createObjectURL(
    new Blob([json], { type: 'application/json' }),
  );
  const link = document.createElement('a');
  link.href = url;
  link.download = name;
  link.click();
  URL.revokeObjectURL(url);
}

function Reveal({
  answered,
  full,
  onKeep,
  onLeave,
}: {
  answered: Answered;
  full: boolean;
  onKeep: (mark: string) => void;
  onLeave: () => void;
}) {
  const judged = answered.exchange.judgement.verdict_correct;
  const [word, tone] =
    judged === true
      ? ['Right.', 'right']
      : judged === false
        ? ['Wrong.', 'wrong']
        : ['You held back.', 'held'];
  return (
    <div className="reveal">
      <p className={tone}>
        {word}{' '}
        {answered.reported.compiles
          ? 'It compiles.'
          : 'The compiler rejects it.'}
        <span className="points">{signed(answered.points)}</span>
      </p>
      <Claims exchange={answered.exchange} />
      <CompilerSays reported={answered.reported} />
      {full ? (
        <div className="choices">
          <span className="ask">Your reasoning was</span>
          <button onClick={() => onKeep('sound')} type="button">
            Sound
          </button>
          <button onClick={() => onKeep('partial')} type="button">
            Partial
          </button>
          <button onClick={() => onKeep('wrong')} type="button">
            Wrong
          </button>
          <button className="quiet" onClick={() => onKeep('')} type="button">
            Skip
          </button>
          <button className="quiet leave" onClick={onLeave} type="button">
            Leave
          </button>
        </div>
      ) : (
        <div className="choices">
          <button onClick={() => onKeep('')} type="button">
            Next
          </button>
          <button className="quiet leave" onClick={onLeave} type="button">
            Leave
          </button>
        </div>
      )}
    </div>
  );
}

function Question({
  phase,
  reasoning,
  onReasoning,
  onAnswer,
  onKeep,
  onLeave,
}: {
  phase: Posed;
  reasoning: string;
  onReasoning: (text: string) => void;
  onAnswer: (said: Said) => void;
  onKeep: (mark: string) => void;
  onLeave: () => void;
}) {
  const { live, next, shown } = phase;
  const asked =
    phase.kind === 'asking' ? live.history.length + 1 : live.history.length;
  // In practice the count is known; in a mastery sitting the end is the
  // model's to call, and no meter is shown along the way, since a visible
  // one gets played to.
  const progress = live.knobs.practice
    ? `Case ${asked} of ${live.knobs.budget}`
    : `Case ${asked}`;
  // The two verdicts swap places from case to case, keyed by the case, so
  // the same button cannot be pressed without reading.
  const verdicts: [Said, string][] = [
    ['compiles', 'It compiles'],
    ['rejected', 'It is rejected'],
  ];
  const ordered =
    next.prompt.seed % 2n === 1n ? [verdicts[1], verdicts[0]] : verdicts;
  return (
    <section className="question">
      <p className="progress">{progress}</p>
      {phase.kind === 'asking' && next.teach !== null && (
        <Teach rule={next.teach} />
      )}
      {shown.map((file) => (
        <Code key={file.name} source={file.source} />
      ))}
      {phase.kind === 'asking' && (
        <>
          {live.full && (
            <label className="why">
              Why?
              <textarea
                onChange={(e) => onReasoning(e.target.value)}
                placeholder="What does this case turn on?"
                rows={3}
                value={reasoning}
              />
            </label>
          )}
          <div className="choices">
            {ordered.map(([said, label]) => (
              <button key={said} onClick={() => onAnswer(said)} type="button">
                {label}
              </button>
            ))}
            <button
              className="quiet"
              onClick={() => onAnswer('unsure')}
              type="button"
            >
              Not sure
            </button>
            <button className="quiet leave" onClick={onLeave} type="button">
              Leave
            </button>
          </div>
          <PointsRule points={pointsOf(live.knobs)} />
        </>
      )}
      {phase.kind === 'revealed' && (
        <Reveal
          answered={phase.answered}
          full={live.full}
          onKeep={onKeep}
          onLeave={onLeave}
        />
      )}
    </section>
  );
}

export default function App() {
  const [phase, setPhase] = useState<Phase>({ kind: 'home' });
  const [slots, setSlots] = useState<Slot[]>(() => readSlots(storage));
  const [ready, setReady] = useState(false);
  const [reasoning, setReasoning] = useState('');
  const [notice, setNotice] = useState<string | null>(null);

  useEffect(() => {
    loadFormatter().then(
      () => setReady(true),
      (error: unknown) =>
        setNotice(`the formatter would not load: ${String(error)}`),
    );
  }, []);

  const store = useCallback((live: Live) => {
    writeSlot(storage, live.slot, saveOf(live));
    setSlots(readSlots(storage));
  }, []);

  /** Move to the phase `act` works out, or say why that could not be done. */
  const move = useCallback((doing: string, act: () => Phase) => {
    try {
      setPhase(act());
    } catch (error) {
      setPhase({ kind: 'broken', message: `${doing}: ${String(error)}` });
    }
  }, []);

  const home = useCallback(() => {
    setSlots(readSlots(storage));
    setPhase({ kind: 'home' });
  }, []);

  const begin = useCallback(
    (slot: number, chosen: Chosen) =>
      move('starting the sitting', () => {
        const live: Live = {
          full: chosen.full,
          history: [],
          knobs: knobsFrom(chosen.knobs),
          profile: freshProfile(),
          session: Math.floor(Math.random() * 2 ** 31),
          slot,
        };
        store(live);
        setReasoning('');
        return advance(live);
      }),
    [move, store],
  );

  const resume = useCallback(
    (slot: number, save: Save) =>
      move('resuming the sitting', () => {
        setReasoning('');
        return advance(fromSave(slot, save));
      }),
    [move],
  );

  // The answer is kept, unmarked, the moment it is given: a learner who
  // leaves at the reveal has still answered, and coming back must not show
  // them the same case now that they have seen its answer.
  const answer = useCallback(
    (said: Said) => {
      if (phase.kind !== 'asking') {
        return;
      }
      const { live, next, shown } = phase;
      move('answering', () => {
        const what = given(said, reasoning, '');
        const answered = answerCase(
          live.profile,
          live.knobs,
          next.prompt.item,
          next.prompt.seed,
          what,
        );
        const after: Live = {
          ...live,
          history: [
            ...live.history,
            taken(next.prompt.item, next.prompt.seed, what),
          ],
          profile: answered.profile,
        };
        store(after);
        return { answered, kind: 'revealed', live: after, next, said, shown };
      });
    },
    [phase, reasoning, move, store],
  );

  // In full mode the learner marks their own reasoning against the
  // derivation they have just been shown. The mark is kept for the
  // transcript and is not evidence: the profile already moved on the verdict.
  const keep = useCallback(
    (mark: string) => {
      if (phase.kind !== 'revealed') {
        return;
      }
      const { live, said } = phase;
      move('keeping the answer', () => {
        const last = live.history.at(-1);
        if (last === undefined) {
          throw new Error('nothing was answered');
        }
        const marked = taken(
          last.item,
          last.seed,
          given(said, reasoning, mark),
        );
        const after: Live = {
          ...live,
          history: [...live.history.slice(0, -1), marked],
        };
        store(after);
        setReasoning('');
        return advance(after);
      });
    },
    [phase, reasoning, move, store],
  );

  const exportHistory = useCallback(
    (session: number, full: boolean, knobs: Knobs, history: Taken[]) => {
      try {
        download(
          `type-quiz-${session}.json`,
          transcriptJson(session, full, knobs, history),
        );
      } catch (error) {
        setNotice(`could not export the sitting: ${String(error)}`);
      }
    },
    [],
  );

  const exportSave = useCallback(
    (save: Save) => {
      const { knobs, history } = heldBy(save);
      exportHistory(save.session, save.full, knobs, history);
    },
    [exportHistory],
  );

  const reset = useCallback((slot: number) => {
    clearSlot(storage, slot);
    setSlots(readSlots(storage));
  }, []);

  return (
    <main className="shell">
      <header>
        <h1>BAML type-system quiz</h1>
        <p className="blurb">
          Every program below was generated from the rules in TYPE_SYSTEM.md and
          checked against the real compiler. Say whether it compiles, or that
          you are not sure.
        </p>
      </header>

      {notice !== null && <p className="notice">{notice}</p>}

      {phase.kind === 'home' && (
        <Home
          onExport={exportSave}
          onNew={(slot) => setPhase({ kind: 'setup', slot })}
          onReset={reset}
          onResume={resume}
          slots={slots}
        />
      )}

      {phase.kind === 'setup' && (
        <Setup
          onBack={home}
          onStart={(chosen) => begin(phase.slot, chosen)}
          ready={ready}
        />
      )}

      {(phase.kind === 'asking' || phase.kind === 'revealed') && (
        <Question
          onAnswer={answer}
          onKeep={keep}
          onLeave={home}
          onReasoning={setReasoning}
          phase={phase}
          reasoning={reasoning}
        />
      )}

      {phase.kind === 'done' && (
        <section className="done">
          <h2>Done</h2>
          <Readout
            knobs={phase.live.knobs}
            profile={phase.live.profile}
            standing={phase.standing}
            starved={phase.starved}
          />
          <div className="choices">
            <button
              disabled={phase.live.history.length === 0}
              onClick={() =>
                exportHistory(
                  phase.live.session,
                  phase.live.full,
                  phase.live.knobs,
                  phase.live.history,
                )
              }
              type="button"
            >
              Download the sitting
            </button>
            <button className="quiet" onClick={home} type="button">
              Back to the slots
            </button>
          </div>
          <p className="note">
            The download is what <code>baml run review</code> reads back, case
            by case, to check every one of them against the compiler again.
          </p>
        </section>
      )}

      {phase.kind === 'broken' && (
        <section>
          <p className="notice">{phase.message}</p>
          <button className="quiet" onClick={home} type="button">
            Back to the slots
          </button>
        </section>
      )}
    </main>
  );
}
