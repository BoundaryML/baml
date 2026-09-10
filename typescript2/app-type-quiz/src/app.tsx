import { useCallback, useEffect, useMemo, useState } from 'react';
import { formatSource, loadFormatter } from './format';
import { highlight } from './highlight';
import { segments } from './prose';
import {
  answerAt,
  compilerReport,
  type Exchange,
  type Given,
  given,
  type Prompt,
  promptAt,
  type Reported,
  type Score,
  type Sitting,
  score,
  sittingLength,
  transcriptJson,
} from './quiz';

/** Whether the learner is asked why, as well as whether. */
type Mode = 'simple' | 'full';

/** Where a sitting has got to. */
type Phase =
  | { kind: 'setup' }
  | { kind: 'asking'; index: number; prompt: Prompt; source: string }
  | {
      kind: 'revealed';
      index: number;
      prompt: Prompt;
      source: string;
      compiles: boolean;
      exchange: Exchange;
      reported: Reported | null;
    }
  | { kind: 'done' };

function Code({ source }: { source: string }) {
  const html = useMemo(() => highlight(source), [source]);
  return (
    <pre className="case">
      {/* biome-ignore lint/security/noDangerouslySetInnerHtml: highlight.js
          returns HTML, and it is the only way to render its token spans. The
          input is a program this app generated, never anything a user typed,
          and highlight.js escapes the source it wraps. */}
      <code dangerouslySetInnerHTML={{ __html: html }} />
    </pre>
  );
}

/** Prose from BAML, with the types it names set as code. */
function Prose({ text }: { text: string }) {
  return (
    <>
      {segments(text).map((segment, at) =>
        segment.code ? (
          <code key={`${at}-${segment.text}`}>{segment.text}</code>
        ) : (
          <span key={`${at}-${segment.text}`}>{segment.text}</span>
        ),
      )}
    </>
  );
}

function Claims({ exchange }: { exchange: Exchange }) {
  return (
    <ol className="claims">
      {exchange.question.case.trace.map((claim, at) => (
        <li key={`${claim.rule.name}-${at}`}>
          <span className="instance">
            <Prose text={claim.instance} />
          </span>
          <span className="rule">{claim.rule.name}</span>
        </li>
      ))}
    </ol>
  );
}

function CompilerSays({ reported }: { reported: Reported | null }) {
  if (reported === null) {
    return null;
  }
  if (reported.compiles && reported.messages.length === 0) {
    return <p className="compiler">The compiler accepts it.</p>;
  }
  return (
    <div className="compiler">
      <p>The compiler says:</p>
      <ul>
        {reported.messages.map((message, at) => (
          <li key={`${reported.codes[at] ?? at}`}>
            <code className="code">{reported.codes[at] ?? ''}</code>{' '}
            <Prose text={message} />
          </li>
        ))}
      </ul>
    </div>
  );
}

function Summary({
  sitting,
  full,
  answers,
}: {
  sitting: Sitting;
  full: boolean;
  answers: Given[];
}) {
  const counted: Score = useMemo(
    () => score(sitting, full, answers),
    [sitting, full, answers],
  );
  return (
    <table className="score">
      <tbody>
        <tr>
          <th>Verdicts right</th>
          <td>
            {counted.verdicts_right} of {counted.asked}
          </td>
        </tr>
        {full && (
          <tr>
            <th>Reasoning</th>
            <td>
              {counted.sound} sound, {counted.partial} partial, {counted.wrong}{' '}
              wrong, {counted.unmarked} unmarked
            </td>
          </tr>
        )}
      </tbody>
    </table>
  );
}

export default function App() {
  const [mode, setMode] = useState<Mode>('simple');
  const [seed, setSeed] = useState(() => Math.floor(Math.random() * 100000));
  const [length, setLength] = useState(10);
  const [sitting, setSitting] = useState<Sitting | null>(null);
  const [total, setTotal] = useState(0);
  const [answers, setAnswers] = useState<Given[]>([]);
  const [reasoning, setReasoning] = useState('');
  const [phase, setPhase] = useState<Phase>({ kind: 'setup' });
  const [ready, setReady] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const full = mode === 'full';

  useEffect(() => {
    loadFormatter().then(
      () => setReady(true),
      (error: unknown) =>
        setFailure(`the formatter would not load: ${String(error)}`),
    );
  }, []);

  const show = useCallback((at: Sitting, index: number, count: number) => {
    const prompt = promptAt(at, index);
    if (prompt === null || index >= count) {
      setPhase({ kind: 'done' });
      return;
    }
    setReasoning('');
    setPhase({
      index,
      kind: 'asking',
      prompt,
      source: formatSource(prompt.files['case.baml'] ?? ''),
    });
  }, []);

  const start = useCallback(() => {
    try {
      const at = { length, seed };
      const count = sittingLength(at);
      setSitting(at);
      setTotal(count);
      setAnswers([]);
      show(at, 0, count);
    } catch (error) {
      setFailure(`could not start: ${String(error)}`);
    }
  }, [seed, length, show]);

  const answer = useCallback(
    (compiles: boolean) => {
      if (phase.kind !== 'asking' || sitting === null) {
        return;
      }
      const exchange = answerAt(
        sitting,
        phase.index,
        given(compiles, reasoning, ''),
      );
      if (exchange === null) {
        setPhase({ kind: 'done' });
        return;
      }
      setPhase({
        compiles,
        exchange,
        index: phase.index,
        kind: 'revealed',
        prompt: phase.prompt,
        reported: compilerReport(sitting, phase.index),
        source: phase.source,
      });
    },
    [phase, sitting, reasoning],
  );

  // In full mode the learner marks their own reasoning against the derivation
  // they have just been shown. Only what they said is kept; the judgement is
  // the engine's to make, and it makes it again from this when it is asked.
  const keep = useCallback(
    (mark: string) => {
      if (phase.kind !== 'revealed' || sitting === null) {
        return;
      }
      setAnswers([...answers, given(phase.compiles, reasoning, mark)]);
      show(sitting, phase.index + 1, total);
    },
    [phase, sitting, answers, reasoning, total, show],
  );

  const download = useCallback(() => {
    if (sitting === null) {
      return;
    }
    const json = transcriptJson(sitting, full, answers);
    const url = URL.createObjectURL(
      new Blob([json], { type: 'application/json' }),
    );
    const link = document.createElement('a');
    link.href = url;
    link.download = `type-quiz-${mode}-${sitting.seed}.json`;
    link.click();
    URL.revokeObjectURL(url);
  }, [mode, full, sitting, answers]);

  if (failure !== null) {
    return (
      <main className="shell">
        <p className="failure">{failure}</p>
      </main>
    );
  }

  return (
    <main className="shell">
      <header>
        <h1>BAML type-system quiz</h1>
        <p className="blurb">
          Every program below was generated from the rules in TYPE_SYSTEM.md and
          checked against the real compiler. Say whether it compiles.
        </p>
      </header>

      {phase.kind === 'setup' && (
        <section className="setup">
          <label>
            Mode
            <select
              onChange={(e) => setMode(e.target.value as Mode)}
              value={mode}
            >
              <option value="simple">Simple — does it compile?</option>
              <option value="full">Full — and why?</option>
            </select>
          </label>
          <label>
            Seed
            <input
              onChange={(e) => setSeed(Number(e.target.value))}
              type="number"
              value={seed}
            />
          </label>
          <label>
            Questions
            <input
              max={50}
              min={1}
              onChange={(e) => setLength(Number(e.target.value))}
              type="number"
              value={length}
            />
          </label>
          <button disabled={!ready} onClick={start} type="button">
            {ready ? 'Start' : 'Loading…'}
          </button>
          <p className="note">
            The same seed asks the same questions, in the same order.
          </p>
        </section>
      )}

      {(phase.kind === 'asking' || phase.kind === 'revealed') && (
        <section className="question">
          <p className="progress">
            Case {phase.index + 1} of {total}
          </p>
          <Code source={phase.source} />

          {phase.kind === 'asking' && (
            <>
              {full && (
                <label className="why">
                  Why?
                  <textarea
                    onChange={(e) => setReasoning(e.target.value)}
                    placeholder="What does this case turn on?"
                    rows={3}
                    value={reasoning}
                  />
                </label>
              )}
              <div className="choices">
                <button onClick={() => answer(true)} type="button">
                  It compiles
                </button>
                <button onClick={() => answer(false)} type="button">
                  It is rejected
                </button>
              </div>
            </>
          )}

          {phase.kind === 'revealed' && (
            <div className="reveal">
              <p
                className={
                  phase.exchange.judgement.verdict_correct ? 'right' : 'wrong'
                }
              >
                {phase.exchange.judgement.verdict_correct ? 'Right.' : 'Wrong.'}{' '}
                {phase.reported?.compiles
                  ? 'It compiles.'
                  : 'The compiler rejects it.'}
              </p>
              <Claims exchange={phase.exchange} />
              <CompilerSays reported={phase.reported} />
              {full ? (
                <div className="choices">
                  <span className="ask">Your reasoning was</span>
                  <button onClick={() => keep('sound')} type="button">
                    Sound
                  </button>
                  <button onClick={() => keep('partial')} type="button">
                    Partial
                  </button>
                  <button onClick={() => keep('wrong')} type="button">
                    Wrong
                  </button>
                  <button
                    className="quiet"
                    onClick={() => keep('')}
                    type="button"
                  >
                    Skip
                  </button>
                </div>
              ) : (
                <div className="choices">
                  <button onClick={() => keep('')} type="button">
                    Next
                  </button>
                </div>
              )}
            </div>
          )}
        </section>
      )}

      {phase.kind === 'done' && sitting !== null && (
        <section className="done">
          <h2>Done</h2>
          <Summary answers={answers} full={full} sitting={sitting} />
          <div className="choices">
            <button
              disabled={answers.length === 0}
              onClick={download}
              type="button"
            >
              Download the sitting
            </button>
            <button
              className="quiet"
              onClick={() => setPhase({ kind: 'setup' })}
              type="button"
            >
              Again
            </button>
          </div>
          <p className="note">
            The download is what <code>baml run review</code> reads back, case
            by case, to check every one of them against the compiler again.
          </p>
        </section>
      )}
    </main>
  );
}
