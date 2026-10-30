/** biome-ignore-all lint/suspicious/noArrayIndexKey: the install steps are static lines */
'use client';

import { type MouseEvent, useId, useState } from 'react';
import { sectionOf, track } from '../track';
import useTabKeys from '../use-tab-keys';

const TAB_ORDER = ['agents', 'humans'] as const;

// The live site's install steps (boundaryml.com). "for agents" is a prompt to paste into an agent;
// "for humans" is the same thing as plain shell.
const TABS = {
  agents: `Set up BAML in this repo, then write and run a function.
1. brew install baml
2. baml init
3. baml agent install
4. baml run main`,
  humans: `brew install baml
# or: curl -fsSL https://pkg.boundaryml.com/install.sh | sh -s
baml init
baml agent install
baml run main`,
} as const;

type Tab = keyof typeof TABS;

export default function Install({ className = '' }: { className?: string }) {
  const [tab, setTab] = useState<Tab>('agents');
  const [copied, setCopied] = useState(false);
  // The panel appears in both the hero and the closing band, so the tab ids have to be per-instance.
  const uid = useId();
  const keys = useTabKeys(TAB_ORDER, tab, setTab);

  const copy = (e: MouseEvent<HTMLButtonElement>) => {
    track('install_copied', { section: sectionOf(e.currentTarget), tab });
    // A blocked clipboard (insecure context, denied permission) leaves the label as it was.
    navigator.clipboard?.writeText(TABS[tab]).then(
      () => {
        setCopied(true);
        setTimeout(() => setCopied(false), 1500);
      },
      () => {},
    );
  };

  return (
    <div
      className={`flex flex-col rounded-xl bg-raised border border-rule overflow-hidden ${className}`}
    >
      <div className="flex items-center justify-between p-2 border-b border-rule text-[13px] leading-4">
        <div
          aria-label="Install instructions"
          className="flex gap-1"
          onKeyDown={keys.onKeyDown}
          role="tablist"
        >
          {TAB_ORDER.map((t) => (
            <button
              aria-controls={`${uid}-panel`}
              aria-selected={tab === t}
              className={`tap px-2.5 py-1.5 rounded-md transition-colors ${
                tab === t
                  ? 'bg-clover/15 text-chip-ink'
                  : 'text-muted hover:text-body'
              }`}
              id={`${uid}-tab-${t}`}
              key={t}
              onClick={(e) => {
                track('install_tab_selected', {
                  section: sectionOf(e.currentTarget),
                  tab: t,
                });
                setTab(t);
              }}
              role="tab"
              tabIndex={keys.tabIndex(t)}
              type="button"
            >
              for {t}
            </button>
          ))}
        </div>
        <button
          aria-live="polite"
          className="tap px-2.5 py-1.5 text-muted hover:text-ink transition-colors"
          onClick={copy}
          type="button"
        >
          {copied ? 'Copied' : tab === 'agents' ? 'Copy prompt' : 'Copy'}
        </button>
      </div>
      {/* Same hanging indent as the other code panes, so a wrapped step reads as one step. */}
      <pre
        aria-labelledby={`${uid}-tab-${tab}`}
        className="px-[18px] pt-4 pb-[18px] font-mono text-[12.5px] leading-5 text-ink-2 break-words"
        id={`${uid}-panel`}
        role="tabpanel"
      >
        {TABS[tab].split('\n').map((l, i) => (
          <div className="code-line" key={i}>
            {l || ' '}
          </div>
        ))}
      </pre>
    </div>
  );
}
