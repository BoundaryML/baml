// The pieces of a question and its reveal, each a view of a BAML value.

import { useMemo } from 'react';
import { highlight } from './highlight';
import { segments } from './prose';
import type { Claim, Points, Reported, Rule, Verdicted } from './quiz';

export function Code({ source }: { source: string }) {
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
export function Prose({ text }: { text: string }) {
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

export function Claims({ claims }: { claims: Claim[] }) {
  if (claims.length === 0) {
    return null;
  }
  return (
    <ol className="claims">
      {claims.map((claim, at) => (
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

/**
 * One program of a question. Before an answer it is bare, and labelled only
 * when it is one of two, since a lone program needs no name to be referred
 * to. After one it carries what the compiler made of it and what that turns
 * on, which for two programs is the part where they differ.
 */
export function Program({
  files,
  label,
  verdicted,
}: {
  files: { name: string; source: string }[];
  label: string | null;
  verdicted: Verdicted | null;
}) {
  return (
    <section className="program">
      {label !== null && (
        <p className="label">
          {label}
          {verdicted !== null && (
            <span className={verdicted.compiles ? 'right' : 'wrong'}>
              {verdicted.compiles ? ' — compiles' : ' — rejected'}
            </span>
          )}
        </p>
      )}
      {files.map((file) => (
        <Code key={file.name} source={file.source} />
      ))}
      {verdicted !== null && (
        <>
          <Claims claims={verdicted.claims} />
          <CompilerSays reported={verdicted.reported} />
        </>
      )}
    </section>
  );
}

export function CompilerSays({ reported }: { reported: Reported }) {
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

/**
 * The spec's own words on a rule the learner keeps missing, shown before a
 * case that turns on it.
 */
export function Teach({ rule }: { rule: Rule }) {
  const where =
    'heading' in rule.spec
      ? rule.spec.heading
      : `SPEC_GAPS.md, ${rule.spec.id}`;
  return (
    <aside className="teach">
      <p className="ask">
        This one turns on <code>{rule.name}</code>, which you have missed twice
        running. The spec, under "{where}":
      </p>
      <blockquote>
        {[rule.quote, ...rule.more].map((quote) => (
          <p key={quote}>
            <Prose text={quote} />
          </p>
        ))}
      </blockquote>
    </aside>
  );
}

/**
 * The scoring rule, shown whole. It is derived from the abstain bar, so
 * answering beats holding back exactly when the learner is surer than the
 * bar: the honest play is the best one, and there is nothing to game.
 */
export function PointsRule({ points }: { points: Points }) {
  return (
    <p className="rule-line">
      Right <b>{signed(points.right)}</b> · wrong <b>{signed(points.wrong)}</b>{' '}
      · not sure <b>{signed(points.unsure)}</b>
    </p>
  );
}

/** A number with its sign, and a real minus. */
export function signed(n: number): string {
  if (n > 0) {
    return `+${n}`;
  }
  if (n < 0) {
    return `−${-n}`;
  }
  return '0';
}

/** A share as a whole percentage. */
export function percent(share: number): string {
  return `${Math.round(share * 100)}%`;
}
