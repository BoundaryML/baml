import Image from 'next/image';
import { Children, isValidElement, type ReactNode } from 'react';
import {
  type LanguageTabName,
  languageTabDefinition,
} from '@/lib/content/language-tabs';

/** Show the reader's familiar idiom and its BAML equivalent together. */
export function CodeComparison({
  children,
  from,
}: {
  children: ReactNode;
  from: LanguageTabName;
}) {
  const panels = Children.toArray(children).filter(isValidElement);
  if (panels.length !== 2)
    throw new Error('CodeComparison requires two code panels');
  const languages: LanguageTabName[] = [from, 'BAML'];
  return (
    <div className="code-comparison">
      {languages.map((language, index) => (
        <section
          aria-label={`${language} equivalent`}
          className="code-comparison-panel"
          key={language}
        >
          <div className="code-comparison-label not-typeset">
            <Image
              alt=""
              height={16}
              src={languageTabDefinition(language).logo}
              width={16}
            />
            {language}
          </div>
          {panels[index]}
        </section>
      ))}
    </div>
  );
}
