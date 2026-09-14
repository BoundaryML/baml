import { useCallback, useEffect, useRef, useState } from 'react';
import { formatSource, loadFormatter } from './format';
import { Home } from './home';
import {
  type Grade,
  type JudgeSettings,
  judgeReady,
  judgeReasoning,
  loadJudge,
} from './judge';
import { Claims, PointsRule, Program, Prose, signed, Teach } from './panels';
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
  type Prompt,
  pointsOf,
  type Revealed,
  replay,
  type Said,
  type Standing,
  standingOf,
  type Taken,
  taken,
  transcriptJson,
  type Verdicted,
  Why,
  wordsFor,
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

/** One program of a question: its files, laid out by the formatter. */
type Shown = { name: string; source: string }[];

/** The programs a prompt shows, in the order it shows them. */
function shownOf(prompt: Prompt): Shown[] {
  const programs = [prompt.program];
  if (prompt.against !== null) {
    programs.push(prompt.against);
  }
  return programs.map((files) =>
    Object.entries(files).map(([name, source]) => ({
      name,
      source: formatSource(source),
    })),
  );
}

/**
 * What each program is called when there are two to tell apart. A lone
 * program needs no name: nothing has to be said to refer to it.
 */
const LABELS = ['First', 'Second'];

function labelOf(at: number, count: number): string | null {
  return count > 1 ? (LABELS[at] ?? `Program ${at + 1}`) : null;
}

/** Where the page has got to. */
type Phase =
  | { kind: 'home' }
  | { kind: 'setup'; slot: number }
  | { kind: 'asking'; live: Live; next: Next; shown: Shown[] }
  | {
      kind: 'why';
      live: Live;
      next: Next;
      shown: Shown[];
      said: Said;
      why: string;
    }
  | {
      kind: 'revealed';
      live: Live;
      next: Next;
      shown: Shown[];
      said: Said;
      why: string;
      answered: Answered;
    }
  | { kind: 'done'; live: Live; standing: Standing; starved: boolean }
  | { kind: 'broken'; message: string };

type Posed = Extract<Phase, { kind: 'asking' | 'why' | 'revealed' }>;

/**
 * Whether an answer is one with anything to explain.
 *
 * A program the compiler accepts breaks no rule, and the rules it does not
 * break are not a reason: asking why it compiles gets the learner restating
 * the program. What has a reason is a rejection — one rule, named, that the
 * program runs into — so that is what is asked for, on the one program that
 * was rejected and on the one of two the learner says fails. Holding back has
 * nothing to explain by definition.
 */
function asksWhy(said: Said, programs: number): boolean {
  if (said === 'unsure') {
    return false;
  }
  return programs > 1 || said === 'rejected';
}

/** The program the learner's answer says the compiler rejects. */
function failingOne(said: Said): string {
  return said === 'first' ? 'second' : 'first';
}

const storage = window.localStorage;

/**
 * The session a new sitting is drawn from. `?session=N` fixes it, so a
 * sitting can be asked for again exactly: the same session and the same
 * answers ask the same questions in the same order.
 */
function newSession(): number {
  const asked = new URLSearchParams(window.location.search).get('session');
  const given = asked === null ? Number.NaN : Number(asked);
  return Number.isSafeInteger(given) && given >= 0
    ? given
    : Math.floor(Math.random() * 2 ** 31);
}

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
    marked_by: kept.given.marked_by,
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
      taken(
        a.item,
        BigInt(a.seed),
        given(a.said, a.reasoning, a.mark, a.marked_by ?? ''),
      ),
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
  return { kind: 'asking', live, next, shown: shownOf(next.prompt) };
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

/**
 * Where a case's reasoning stands with its marker.
 *
 * The judge is the marker whenever there is one to ask, so `asking` is where
 * a case with a key and a reason begins; `by hand` is what is left when there
 * is no judge, and `nothing` is a case with no reasoning to mark at all. A
 * learner who has set a key is never asked to mark their own reasoning:
 * marking is the thing they set it to have done. Holding the grade inside the
 * judged state rather than beside it keeps a mark without a judgement, or a
 * judgement still in flight, unrepresentable.
 */
type Judging =
  | { kind: 'asking' }
  | { kind: 'judged'; grade: Grade; mark: string }
  | { kind: 'failed'; why: string }
  | { kind: 'by hand' }
  | { kind: 'nothing' };

/**
 * Where a reveal starts: who is to mark this case, if anyone.
 *
 * A reason is the whole condition. Only a sitting that asks why can produce
 * one, so there is nothing to check about the sitting here: a case with a
 * reason has one to mark, and a case without has nothing to mark whatever
 * mode it was answered in.
 */
function marker(judge: JudgeSettings, reasoning: string): Judging {
  if (reasoning.trim() === '') {
    return { kind: 'nothing' };
  }
  return judgeReady(judge) ? { kind: 'asking' } : { kind: 'by hand' };
}

function Reveal({
  revealed,
  said,
  reasoning,
  onKeep,
  onLeave,
}: {
  revealed: Revealed;
  said: Said;
  reasoning: string;
  onKeep: (mark: string, markedBy?: string) => void;
  onLeave: () => void;
}) {
  // The shared claims only, and only when there is more than one program to
  // share them: with one, they were shown under it.
  const shared = revealed.programs.length > 1 ? revealed.shared : [];
  const [judge] = useState(loadJudge);
  const [judging, setJudging] = useState<Judging>(() =>
    marker(judge, reasoning),
  );
  // Asked from an effect rather than during the render that reaches
  // `asking`, so a second render — which is every render in development —
  // cannot spend the key twice on one case.
  const asked = useRef(false);
  useEffect(() => {
    if (judging.kind !== 'asking' || asked.current) {
      return;
    }
    asked.current = true;
    const ask = async () => {
      try {
        const judged = await judgeReasoning(judge, revealed, said, reasoning);
        // Only if the learner has not moved on in the meantime.
        setJudging((was) => {
          if (was.kind !== 'asking') {
            return was;
          }
          return judged.kind === 'grade'
            ? { grade: judged.grade, kind: 'judged', mark: judged.mark }
            : judged;
        });
      } catch (error: unknown) {
        setJudging((was) =>
          was.kind === 'asking' ? { kind: 'failed', why: String(error) } : was,
        );
      }
    };
    void ask();
  }, [judging.kind, judge, revealed, said, reasoning]);
  const again = () => {
    asked.current = false;
    setJudging({ kind: 'asking' });
  };
  return (
    <div className="reveal">
      <Claims claims={shared} />
      {judging.kind === 'judged' && (
        <div className="judged">
          <p className="mark">
            The judge marks this <strong>{judging.mark}</strong>.
          </p>
          <p>
            <Prose text={judging.grade.feedback} />
          </p>
          {judging.grade.missed.length > 0 && (
            <>
              <p className="ask">What the reasoning did not reach</p>
              <ul>
                {judging.grade.missed.map((line) => (
                  <li key={line}>
                    <Prose text={line} />
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
      )}
      {judging.kind === 'failed' && (
        <p className="notice">The judge could not answer. {judging.why}</p>
      )}
      <div className="choices">
        {judging.kind === 'asking' && <span className="ask">Judging…</span>}
        {judging.kind === 'judged' && (
          <button
            onClick={() => onKeep(judging.mark, judge.model)}
            type="button"
          >
            Next
          </button>
        )}
        {judging.kind === 'failed' && (
          <button onClick={again} type="button">
            Ask the judge again
          </button>
        )}
        {judging.kind === 'by hand' && (
          <>
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
          </>
        )}
        {judging.kind !== 'judged' && (
          <button
            className={judging.kind === 'nothing' ? undefined : 'quiet'}
            onClick={() => onKeep('')}
            type="button"
          >
            {judging.kind === 'nothing' ? 'Next' : 'Skip'}
          </button>
        )}
        <button className="quiet leave" onClick={onLeave} type="button">
          Leave
        </button>
      </div>
    </div>
  );
}

/**
 * A program's verdict as the reveal shows it. With one program there is
 * nothing to hold apart, so everything it turns on sits under it; with two,
 * what they share is shown once, below both.
 */
function verdictedAt(
  revealed: Revealed | null,
  at: number,
  programs: number,
): Verdicted | null {
  if (revealed === null) {
    return null;
  }
  const shown = revealed.programs[at];
  return programs > 1 ? shown : { ...shown, claims: revealed.shared };
}

/**
 * How the answer was marked, and what it was worth. Above the programs
 * rather than below them: it is the answer to what was just clicked, and
 * everything under each program is the working.
 */
function Mark({ answered }: { answered: Answered }) {
  const judged = answered.exchange.judgement.verdict_correct;
  const [word, tone] =
    judged === true
      ? ['Right.', 'right']
      : judged === false
        ? ['Wrong.', 'wrong']
        : ['You held back.', 'held'];
  return (
    <p className={`mark ${tone}`}>
      {word} {outcome(answered.revealed)}
      <span className="points">{signed(answered.points)}</span>
    </p>
  );
}

/** What the compiler did, in a sentence, however many programs were shown. */
function outcome(revealed: Revealed): string {
  const compiles = revealed.programs.findIndex((program) => program.compiles);
  if (revealed.programs.length < 2) {
    return compiles === 0 ? 'It compiles.' : 'The compiler rejects it.';
  }
  return compiles === 0
    ? 'The compiler accepts the first.'
    : 'The compiler accepts the second.';
}

function Question({
  phase,
  onReasoning,
  onAnswer,
  onGive,
  onKeep,
  onLeave,
}: {
  phase: Posed;
  onReasoning: (text: string) => void;
  onAnswer: (said: Said) => void;
  onGive: (why: string) => void;
  onKeep: (mark: string, markedBy?: string) => void;
  onLeave: () => void;
}) {
  const { live, next, shown } = phase;
  const answered = phase.kind === 'revealed';
  const asked = answered ? live.history.length : live.history.length + 1;
  // In practice the count is known; in a mastery sitting the end is the
  // model's to call, and no meter is shown along the way, since a visible
  // one gets played to.
  const progress = live.knobs.practice
    ? `Case ${asked} of ${live.knobs.budget}`
    : `Case ${asked}`;
  // The two answers swap places from case to case, keyed by the case, so the
  // same button cannot be pressed without reading. For two programs the
  // words name the programs, whose own order is already drawn, so they stay
  // in the order the programs are in.
  const words = wordsFor(shown.length);
  const ordered =
    shown.length === 1 && next.prompt.seed % 2n === 1n
      ? [words[1], words[0]]
      : words;
  const revealed = phase.kind === 'revealed' ? phase.answered.revealed : null;
  return (
    <section className="question">
      <p className="progress">{progress}</p>
      {shown.length > 1 && !answered && (
        <p className="ask">
          The compiler accepts one of these two and rejects the other. Which one
          does it accept?
        </p>
      )}
      {!answered && next.teach !== null && <Teach rule={next.teach} />}
      {phase.kind === 'revealed' && <Mark answered={phase.answered} />}
      {revealed !== null && revealed.difference !== '' && (
        <p className="difference">
          <Prose text={revealed.difference} />.
        </p>
      )}
      {shown.map((files, at) => (
        <Program
          files={files}
          key={files[0]?.source ?? at}
          label={labelOf(at, shown.length)}
          verdicted={verdictedAt(revealed, at, shown.length)}
        />
      ))}
      {phase.kind === 'asking' && (
        <>
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
      {phase.kind === 'why' && (
        <>
          <label className="why">
            {shown.length > 1
              ? `You said the ${phase.said} compiles. Why does the ${failingOne(phase.said)} fail?`
              : 'Why does the compiler reject it?'}
            <textarea
              onChange={(e) => onReasoning(e.target.value)}
              placeholder="Which rule does it run into?"
              rows={3}
              value={phase.why}
            />
          </label>
          <div className="choices">
            <button
              disabled={phase.why.trim() === ''}
              onClick={() => onGive(phase.why)}
              type="button"
            >
              That is my reasoning
            </button>
            <button className="quiet" onClick={() => onGive('')} type="button">
              Skip the reason
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
          onKeep={onKeep}
          onLeave={onLeave}
          reasoning={phase.why}
          revealed={phase.answered.revealed}
          said={phase.said}
        />
      )}
    </section>
  );
}

export default function App() {
  const [phase, setPhase] = useState<Phase>({ kind: 'home' });
  const [slots, setSlots] = useState<Slot[]>(() => readSlots(storage));
  const [ready, setReady] = useState(false);
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
          session: newSession(),
          slot,
        };
        store(live);
        return advance(live);
      }),
    [move, store],
  );

  const resume = useCallback(
    (slot: number, save: Save) =>
      move('resuming the sitting', () => advance(fromSave(slot, save))),
    [move],
  );

  // The answer is kept, unmarked, the moment it is given: a learner who
  // leaves at the reveal has still answered, and coming back must not show
  // them the same case now that they have seen its answer.
  const answer = useCallback(
    (said: Said, why: string) => {
      if (phase.kind !== 'asking' && phase.kind !== 'why') {
        return;
      }
      const { live, next, shown } = phase;
      move('answering', () => {
        const what = given(said, why, '');
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
        return {
          answered,
          kind: 'revealed',
          live: after,
          next,
          said,
          shown,
          why,
        };
      });
    },
    [phase, move, store],
  );

  // The reason, once the phase holding the answer it explains is the one on
  // screen. The answer travels in the phase rather than beside it, so a
  // reason can never be filed against a different one.
  // The text as it is typed, on the phase that asked for it. A page-wide
  // box would still be holding what was typed after a learner chose not to
  // give it, and the answer, the judge and the transcript would each have to
  // remember to ignore it.
  const writing = useCallback((text: string) => {
    setPhase((was) => (was.kind === 'why' ? { ...was, why: text } : was));
  }, []);

  const give = useCallback(
    (why: string) => {
      if (phase.kind !== 'why') {
        return;
      }
      answer(phase.said, why);
    },
    [phase, answer],
  );

  // What the learner says first is the verdict. A reason is asked for after
  // it and only where there is one to give, so the question they answer is
  // never the question they are about to be asked to explain.
  const say = useCallback(
    (said: Said) => {
      if (phase.kind !== 'asking') {
        return;
      }
      const { live, next, shown } = phase;
      if (live.full && asksWhy(said, shown.length)) {
        setPhase({ kind: 'why', live, next, said, shown, why: '' });
        return;
      }
      answer(said, '');
    },
    [phase, answer],
  );

  // In full mode the learner marks their own reasoning against the
  // derivation they have just been shown. The mark is kept for the
  // transcript and is not evidence: the profile already moved on the verdict.
  const keep = useCallback(
    (mark: string, markedBy = '') => {
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
          given(said, phase.why, mark, markedBy),
        );
        const after: Live = {
          ...live,
          history: [...live.history.slice(0, -1), marked],
        };
        store(after);
        return advance(after);
      });
    },
    [phase, move, store],
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
          ready={ready}
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

      {(phase.kind === 'asking' ||
        phase.kind === 'why' ||
        phase.kind === 'revealed') && (
        <Question
          onAnswer={say}
          onGive={give}
          onKeep={keep}
          onLeave={home}
          onReasoning={writing}
          phase={phase}
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
