'use client';

import { useEffect, useMemo, useState } from 'react';
import type { CSSProperties } from 'react';
import { BAML_SNIPPET } from './constants';
import styles from './styles.module.css';

type CodeMorphProps = {
  ascii: string;
  start: boolean;
};

type Glyph = {
  char: string;
  key: string;
  delay: number;
  driftX: number;
  driftY: number;
};

function tokenClass(token: string) {
  if (/^(function|client|prompt|enum)$/.test(token)) return styles.tokenKeyword;
  if (/^(Category|ClassifyMessage|Greeting|Question|Complaint|Other)$/.test(token))
    return styles.tokenType;
  if (/^".*"$/.test(token) || /^#"$/.test(token) || /^"#$/.test(token))
    return styles.tokenString;
  return styles.tokenPlain;
}

export function CodeMorph({ ascii, start }: CodeMorphProps) {
  const [phase, setPhase] = useState<'ascii' | 'dissolve' | 'code'>('ascii');

  const asciiGlyphs = useMemo<Glyph[]>(() => {
    const chars = ascii.split('');
    return chars.map((char, index) => ({
      char,
      key: `a-${index}`,
      delay: Math.floor(index * 5 + Math.random() * 240),
      driftX: Math.floor(Math.random() * 80 - 40),
      driftY: Math.floor(Math.random() * 120 - 60),
    }));
  }, [ascii]);

  useEffect(() => {
    if (!start) return;
    const dissolveTimer = window.setTimeout(() => setPhase('dissolve'), 2000);
    const codeTimer = window.setTimeout(() => setPhase('code'), 5600);
    return () => {
      window.clearTimeout(dissolveTimer);
      window.clearTimeout(codeTimer);
    };
  }, [start]);

  return (
    <div className={styles.morphRoot}>
      <pre className={styles.morphAscii}>
        <code>
          {asciiGlyphs.map((glyph) => (
            <span
              key={glyph.key}
              className={phase === 'dissolve' || phase === 'code' ? styles.morphGlyphDissolve : styles.morphGlyph}
              style={
                {
                  '--delay': `${glyph.delay}ms`,
                  '--driftX': `${glyph.driftX}px`,
                  '--driftY': `${glyph.driftY}px`,
                } as CSSProperties
              }
            >
              {glyph.char}
            </span>
          ))}
        </code>
      </pre>

      <div className={phase === 'code' ? styles.codeLayerVisible : styles.codeLayer}>
        <pre className={styles.morphCode}>
          {BAML_SNIPPET.split('\n').map((line, lineIndex) => (
            <div key={`line-${lineIndex}`} className={styles.codeLine}>
              {line.split(/(\s+|[{}()":,#-])/g).map((token, tokenIndex) => (
                <span key={`${lineIndex}-${tokenIndex}`} className={tokenClass(token)}>
                  {token}
                </span>
              ))}
            </div>
          ))}
        </pre>
      </div>
    </div>
  );
}
