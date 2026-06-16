'use client';

import { useState } from 'react';
import type { IconType } from 'react-icons';
import { SiGo, SiPython, SiRust, SiTypescript } from 'react-icons/si';
import { BamlCode } from '../../learn2/_components/BamlCode';
import { SDK_BAML, SDK_SNIPPETS, type SdkLang } from './snippets';

const TABS: { id: SdkLang; label: string; Icon: IconType; color: string }[] = [
  { color: '#3776AB', Icon: SiPython, id: 'python', label: 'Python' },
  {
    color: '#3178C6',
    Icon: SiTypescript,
    id: 'typescript',
    label: 'TypeScript',
  },
  { color: '#00ADD8', Icon: SiGo, id: 'go', label: 'Go' },
  { color: '#1a1612', Icon: SiRust, id: 'rust', label: 'Rust' },
];

/**
 * One BAML function on the left; pick a host language on the right and see
 * the generated, typed SDK call. Python/TypeScript ship today; Go and Rust
 * are in progress (badged).
 */
export function SdkSwitcher() {
  const [lang, setLang] = useState<SdkLang>('python');
  const active = SDK_SNIPPETS[lang];

  return (
    <div>
      <div aria-label="SDK language" className="l6-sdk-tabs" role="tablist">
        {TABS.map(({ id, label, Icon, color }) => (
          <button
            aria-selected={lang === id}
            className={`l6-sdk-tab font-mono${lang === id ? ' l6-sdk-tab--on' : ''}`}
            key={id}
            onClick={() => setLang(id)}
            role="tab"
            type="button"
          >
            <Icon aria-hidden color={color} size={14} />
            {label}
            {(id === 'go' || id === 'rust') && (
              <span className="l6-sdk-soon">in progress</span>
            )}
          </button>
        ))}
      </div>
      <div className="l6-pair">
        <div>
          <p className="l6-pane-label">the baml function</p>
          <BamlCode code={SDK_BAML} filename="classify.baml" />
        </div>
        <div>
          <p className="l6-pane-label l6-pane-label--after">
            generated sdk · {active.filename}
          </p>
          <BamlCode
            code={active.code}
            filename={active.filename}
            lang={active.lang}
          />
          <p className="l6-dim" style={{ marginTop: '0.6rem' }}>
            {active.note}
          </p>
        </div>
      </div>
    </div>
  );
}
