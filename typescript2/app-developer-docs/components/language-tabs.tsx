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

export function LanguageTabs({
  children,
  languages,
}: {
  children: ReactNode;
  languages: LanguageTabName[];
}) {
  const id = useId();
  const [selected, setSelected] = useState(0);
  const buttons = useRef<(HTMLButtonElement | null)[]>([]);
  const panels = Children.toArray(children).filter(isValidElement);
  const definitions = languages.map(languageTabDefinition);

  return (
    <div className="language-tabs">
      <div
        aria-label="Comparison language"
        className="language-tabs-list not-typeset"
        role="tablist"
      >
        {languages.map((language, index) => (
          <button
            aria-controls={`${id}-panel-${index}`}
            aria-selected={selected === index}
            id={`${id}-tab-${index}`}
            key={language}
            onClick={() => setSelected(index)}
            onKeyDown={(event) => {
              let next: number;
              switch (event.key) {
                case 'ArrowRight':
                  next = (index + 1) % languages.length;
                  break;
                case 'ArrowLeft':
                  next = (index + languages.length - 1) % languages.length;
                  break;
                case 'Home':
                  next = 0;
                  break;
                case 'End':
                  next = languages.length - 1;
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
            <Image
              alt=""
              className="language-tabs-logo"
              data-monochrome={definitions[index].monochrome}
              height={16}
              src={definitions[index].logo}
              width={16}
            />
            {language}
          </button>
        ))}
      </div>
      {panels.map((panel, index) => (
        <div
          aria-labelledby={`${id}-tab-${index}`}
          className="language-tabs-panel"
          hidden={selected !== index}
          id={`${id}-panel-${index}`}
          key={languages[index]}
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
