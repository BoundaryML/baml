'use client';

import Image from 'next/image';
import {
  Children,
  isValidElement,
  type ReactNode,
  useId,
  useRef,
  useState,
} from 'react';

import {
  type LanguageTabName,
  languageTabDefinition,
} from '@/lib/content/language-tabs';

export interface ExampleTab {
  label: string;
  logo?: string;
  monochrome?: boolean;
}

export function ExampleTabs({
  children,
  label,
  tabs,
}: {
  children: ReactNode;
  label: string;
  tabs: ExampleTab[];
}) {
  const id = useId();
  const [selected, setSelected] = useState(0);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const panels = Children.toArray(children).filter(isValidElement);

  return (
    <div className="language-tabs">
      <div
        aria-label={label}
        className="language-tabs-list not-typeset"
        role="tablist"
      >
        {tabs.map((tab, index) => (
          <button
            aria-controls={`${id}-panel-${index}`}
            aria-selected={selected === index}
            id={`${id}-tab-${index}`}
            key={tab.label}
            onClick={() => setSelected(index)}
            onKeyDown={(event) => {
              let next: number;
              switch (event.key) {
                case 'ArrowRight':
                  next = (index + 1) % tabs.length;
                  break;
                case 'ArrowLeft':
                  next = (index + tabs.length - 1) % tabs.length;
                  break;
                case 'Home':
                  next = 0;
                  break;
                case 'End':
                  next = tabs.length - 1;
                  break;
                default:
                  return;
              }
              event.preventDefault();
              setSelected(next);
              buttons.current[next]?.focus();
            }}
            ref={(element) => {
              buttons.current[index] = element;
            }}
            role="tab"
            tabIndex={selected === index ? 0 : -1}
            type="button"
          >
            {tab.logo ? (
              <Image
                alt=""
                className="language-tabs-logo"
                data-monochrome={tab.monochrome}
                height={16}
                src={tab.logo}
                width={16}
              />
            ) : null}
            {tab.label}
          </button>
        ))}
      </div>
      {panels.map((panel, index) => (
        <div
          aria-labelledby={`${id}-tab-${index}`}
          className="language-tabs-panel"
          hidden={selected !== index}
          id={`${id}-panel-${index}`}
          key={tabs[index].label}
          role="tabpanel"
          // biome-ignore lint/a11y/noNoninteractiveTabindex: Tab lets keyboard readers enter the selected panel's content.
          tabIndex={0}
        >
          {panel}
        </div>
      ))}
    </div>
  );
}

export function LanguageTabs({
  children,
  languages,
}: {
  children: ReactNode;
  languages: LanguageTabName[];
}) {
  return (
    <ExampleTabs
      label="Comparison language"
      tabs={languages.map((language) => ({
        label: language,
        ...languageTabDefinition(language),
      }))}
    >
      {children}
    </ExampleTabs>
  );
}

export function ProviderTabs({ children }: { children: ReactNode }) {
  return (
    <ExampleTabs
      label="Vision model provider"
      tabs={[{ label: 'OpenAI' }, { label: 'Anthropic' }, { label: 'Google' }]}
    >
      {children}
    </ExampleTabs>
  );
}
