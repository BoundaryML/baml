/* eslint-disable @next/next/no-img-element */
'use client';

import { AnimatePresence, motion } from 'motion/react';
import { useState } from 'react';
import { CodeBlock, type CodeSnippet } from './magicui/code-block';

// Constants for better maintainability
const CODE_SNIPPETS = {
  baml: `class Resume {
  name string
  title string
}

function ExtractResume(resume: string) -> Resume`,
  go: `resume := b.ExtractResume(resume)
fmt.Println(resume.Education)`,
  python: `resume = b.ExtractResume(resume)
print(resume.education)`,
  ruby: `resume = b.ExtractResume(resume)
puts resume.education`,
  typescript: `const resume = b.ExtractResume(resume);
console.log(resume.education);`,
} as const;

type Language = 'typescript' | 'python' | 'ruby' | 'go';

const LANGUAGE_CONFIG = {
  go: {
    errorMessage:
      'resume.Education undefined (type Resume has no field or method Education)',
    filename: 'main.go',
    label: 'Go',
  },
  python: {
    errorMessage:
      "AttributeError: 'Resume' object has no attribute 'education'",
    filename: 'main.py',
    label: 'Python',
  },
  ruby: {
    errorMessage: "NoMethodError: undefined method 'education' for Resume",
    filename: 'main.rb',
    label: 'Ruby',
  },
  typescript: {
    errorMessage: "Property 'education' does not exist on type 'Resume'.",
    filename: 'main.ts',
    label: 'TypeScript',
  },
} as const;

export function FirstBentoAnimation() {
  const [showError] = useState(true);
  const [activeLanguage, setActiveLanguage] = useState<Language>('typescript');

  // Add error styling to the education property for each language
  const renderCodeWithError = (language: Language) => {
    switch (language) {
      case 'typescript':
        return (
          <>
            <div>const resume = b.ExtractResume(resume);</div>
            <div>
              console.log(resume
              <span className="error-underline">.education</span>
              );
            </div>
          </>
        );
      case 'python':
        return (
          <>
            <div>resume = b.ExtractResume(resume)</div>
            <div>
              print(resume
              <span className="error-underline">.education</span>)
            </div>
          </>
        );
      case 'ruby':
        return (
          <>
            <div>resume = b.ExtractResume(resume)</div>
            <div>
              puts resume
              <span className="error-underline">.education</span>
            </div>
          </>
        );
      case 'go':
        return (
          <>
            <div>resume := b.ExtractResume(resume)</div>
            <div>
              fmt.Println(resume
              <span className="error-underline">.education</span>)
            </div>
          </>
        );
    }
  };

  // Create snippets array for the multi-file CodeBlock
  const codeSnippets: any[] = (
    ['python', 'typescript', 'ruby', 'go'] as Language[]
  ).map((lang) => ({
    code: (
      <pre className="text-[14px] font-mono text-primary leading-relaxed p-4">
        <code>
          {showError ? renderCodeWithError(lang) : CODE_SNIPPETS[lang]}
        </code>
      </pre>
    ),
    filename: LANGUAGE_CONFIG[lang].filename,
  }));

  return (
    <div className="w-full flex flex-col items-center justify-center gap-6 relative">
      {/* BAML Schema Section */}
      <motion.div
        animate={{ opacity: 1, y: 0 }}
        className="w-full max-w-md"
        initial={{ opacity: 0, y: 30 }}
        transition={{ delay: 0.1, duration: 0.6 }}
      >
        <CodeBlock className="mb-2 shadow-lg mt-2" filename="resume.baml">
          <pre className="text-[14px] font-mono text-primary leading-relaxed p-4">
            <code>{CODE_SNIPPETS.baml}</code>
          </pre>
        </CodeBlock>
      </motion.div>

      {/* Code Section with Integrated Tabs */}
      <motion.div
        animate={{ opacity: 1, y: 0 }}
        className="w-full max-w-md"
        initial={{ opacity: 0, y: 30 }}
        transition={{ delay: 0.3, duration: 0.6 }}
      >
        <CodeBlock
          className="mb-2 relative shadow-lg"
          onTabChange={(index) =>
            setActiveLanguage(
              (Object.keys(LANGUAGE_CONFIG) as Language[])[index],
            )
          }
          snippets={codeSnippets}
        />

        {/* Error message */}
        <AnimatePresence mode="wait">
          {showError && (
            <motion.div
              animate={{ opacity: 1, y: 0 }}
              className="flex items-center gap-2 mt-2 ml-2 mb-6 text-xs text-destructive font-mono"
              exit={{ opacity: 0, y: 10 }}
              initial={{ opacity: 0, y: 10 }}
              key={activeLanguage}
              transition={{ delay: 0.2, duration: 0.4 }}
            >
              <ErrorIcon />
              {LANGUAGE_CONFIG[activeLanguage].errorMessage}
            </motion.div>
          )}
        </AnimatePresence>
      </motion.div>

      {/* Error underline styles */}
      <style jsx>{`
        :global(.error-underline) {
          text-decoration: underline wavy red;
          text-underline-offset: 2px;
          font-weight: 600;
          color: #e11d48;
          background: rgba(255,0,0,0.04);
          border-radius: 2px;
          padding: 0 2px;
        }
      `}</style>
    </div>
  );
}

// Extracted error icon component for better organization
function ErrorIcon() {
  return (
    <svg
      aria-label="Error icon"
      fill="none"
      height="16"
      viewBox="0 0 24 24"
      width="16"
    >
      <title>Error icon</title>
      <circle cx="12" cy="12" r="10" stroke="currentColor" strokeWidth="2" />
      <path
        d="M12 8v4m0 4h.01"
        stroke="currentColor"
        strokeLinecap="round"
        strokeWidth="2"
      />
    </svg>
  );
}
