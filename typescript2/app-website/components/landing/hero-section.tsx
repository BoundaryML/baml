/* eslint-disable @next/next/no-img-element */
/** biome-ignore-all lint/performance/noImgElement: we need to use img for the images */
/* eslint-disable @next/next/no-img-element */
'use client';

import type { LumaEvent } from '@/lib/luma';
import { ScriptCopyBtn } from '../magicui/script-copy-btn';

// Logo section component - exported for use after FeatureSection on homepage
export function LogoSection() {
  const logos = [
    {
      alt: 'OpenAI',
      name: 'OpenAI',
      url: 'https://cdn.brandfetch.io/openai.com/w/512/h/512/logo?c=1idQbe1D_SxVi_WjGRi',
    },
    {
      alt: 'Anthropic',
      name: 'Anthropic',
      url: 'https://cdn.brandfetch.io/anthropic.com/w/512/h/512/logo?c=1idQbe1D_SxVi_WjGRi',
    },
    {
      alt: 'Google',
      name: 'Google',
      url: 'https://cdn.brandfetch.io/google.com/w/501/h/512/symbol?c=1idQbe1D_SxVi_WjGRi',
    },
    {
      alt: 'Microsoft',
      name: 'Microsoft',
      url: 'https://cdn.brandfetch.io/microsoft.com/w/512/h/512/symbol?c=1idQbe1D_SxVi_WjGRi',
    },
    {
      alt: 'Meta',
      name: 'Meta',
      url: 'https://cdn.brandfetch.io/meta.com/w/400/h/400?c=1idQbe1D_SxVi_WjGRi',
    },
  ];

  const frameworks = [
    {
      alt: 'Typescript',
      name: 'Typescript',
      url: 'https://cdn.brandfetch.io/typescriptlang.org/w/256/h/256?c=1idQbe1D_SxVi_WjGRi',
    },
    {
      alt: 'Python',
      name: 'Python',
      url: 'https://cdn.brandfetch.io/python.org/w/467/h/512/logo?c=1idQbe1D_SxVi_WjGRi',
    },
    {
      alt: 'Go',
      name: 'Go',
      url: 'https://cdn.brandfetch.io/go.dev/w/512/h/192/logo?c=1idQbe1D_SxVi_WjGRi',
    },
    {
      alt: 'Ruby',
      name: 'Ruby',
      url: 'https://cdn.brandfetch.io/ruby-lang.org/w/512/h/511/logo?c=1idQbe1D_SxVi_WjGRi',
    },
    {
      alt: 'Java',
      name: 'Java',
      url: 'https://cdn.brandfetch.io/java.com/w/379/h/512/logo?c=1idQbe1D_SxVi_WjGRi',
    },
  ];

  return (
    <div className="mt-16 sm:mt-24 mb-8 translate-y-[-1rem] animate-fade-in opacity-0 [--animation-delay:800ms] w-full px-4 sm:px-8">
      <div className="flex flex-col gap-10 max-w-5xl mx-auto">
        {/* LLM providers */}
        <div>
          <p className="font-medium text-muted-foreground mb-6 tracking-wide text-sm text-center">
            Works with every LLM provider
          </p>
          <div className="flex justify-between items-center">
            {logos.map((logo, index) => (
              <img
                key={logo.name}
                alt={logo.alt}
                src={logo.url}
                className="h-7 sm:h-8 w-auto object-contain grayscale opacity-50 hover:grayscale-0 hover:opacity-100 transition-all duration-300"
                style={{ animationDelay: `${900 + index * 100}ms` }}
              />
            ))}
          </div>
        </div>

        {/* Languages */}
        <div>
          <p className="font-medium text-muted-foreground mb-6 tracking-wide text-sm text-center">
            And every language
          </p>
          <div className="flex justify-between items-center">
            {frameworks.map((framework, index) => (
              <img
                key={framework.name}
                alt={framework.alt}
                src={framework.url}
                className="h-7 sm:h-8 w-auto object-contain grayscale opacity-50 hover:grayscale-0 hover:opacity-100 transition-all duration-300"
                style={{ animationDelay: `${900 + index * 100}ms` }}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

interface HeroSectionProps {
  nextEvent: LumaEvent | null;
}

export default function HeroSection({ nextEvent }: HeroSectionProps) {
  // biome-ignore assist/source/useSortedKeys: needs to be in this order
  const commandMap = {
    python: 'uv add baml-py && uv run baml-cli init',
    typescript: 'npm install @boundaryml/baml && npx baml-cli init',
    ruby: 'bundle add baml && bundle exec baml-cli init',
    go: 'go install github.com/boundaryml/baml/baml-cli@latest && baml-cli init',
    other: (
      <a
        className="inline-flex items-center justify-center rounded-md border border-input bg-background px-4 py-2 text-sm font-medium underline-offset-4 hover:bg-accent hover:text-accent-foreground"
        href="https://docs.boundaryml.com/guide/installation-language/rest-api-other-languages"
      >
        Install BAML
      </a>
    ),
  };

  return (
    <section
      className="grid min-h-[80vh] w-full border-b border-border bg-background md:grid-cols-2"
      id="hero"
    >
      {/* Left column - system outline and CTA */}
      <div className="flex flex-col justify-between border-r border-border px-8 py-12 md:px-12 md:py-16">
        <div>
          <div className="mb-16 flex items-center justify-between text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
            <span>System Outline</span>
            <span>
              10.1 <sup>)</sup>
            </span>
          </div>
          <h1 className="mb-6 text-balance text-[clamp(2.5rem,5vw,4rem)] font-medium leading-[1.1] tracking-[-0.04em]">
            The first language
            <br />
            designed with
            <br />
            LLMs in mind.
          </h1>
          <p className="mb-4 max-w-xl text-sm leading-relaxed text-muted-foreground">
            Build robust AI agents with a programming language that treats LLM
            prompts, schemas, and non-deterministic logic as first-class
            citizens.
          </p>
          <div className="mt-8 flex items-center gap-3">
            <a
              className="inline-flex items-center text-[15px] font-medium tracking-tight transition-colors hover:text-primary"
              href="https://docs.boundaryml.com/?utm_source=marketing-site&utm_medium=hero-cta"
              target="_blank"
              rel="noreferrer"
            >
              Explore documentation
              <svg
                className="ml-3 transition-transform group-hover:translate-x-1"
                height="24"
                stroke="currentColor"
                strokeLinecap="square"
                strokeLinejoin="miter"
                strokeWidth="1.5"
                viewBox="0 0 24 24"
                width="24"
              >
                <path d="M5 12h14M12 5l7 7-7 7" />
              </svg>
            </a>
            <svg
              className="text-[#059669]/60"
              height="20"
              viewBox="0 0 24 24"
              width="20"
            >
              <path d="M12 2C12 2 10 6 10 10C10 14 12 18 12 18C12 18 14 14 14 10C14 6 12 2 12 2ZM12 4.5C13.2 6.5 13.5 8.5 13.5 10C13.5 11.5 13.2 13.5 12 15.5C10.8 13.5 10.5 11.5 10.5 10C10.5 8.5 10.8 6.5 12 4.5ZM6 8C6 8 4 11 4 14C4 17 6 20 6 20C6 20 8 17 8 14C8 11 6 8 6 8ZM18 8C18 8 16 11 16 14C16 17 18 20 18 20C18 20 20 17 20 14C20 11 18 8 18 8Z" />
            </svg>
          </div>
        </div>
        <div className="mt-12 flex items-center justify-between text-[11px] uppercase tracking-[0.18em] text-muted-foreground">
          <span>Origin // 2024</span>
          <span>Open Source</span>
        </div>
      </div>

      {/* Right column - BAML code window */}
      <div
        className="flex items-center justify-center px-6 py-10 md:px-12 md:py-16"
        style={{
          backgroundColor: 'rgba(255,255,255,0.3)',
          backgroundImage:
            "url(\"data:image/svg+xml,%3Csvg width='40' height='40' viewBox='0 0 40 40' xmlns='http://www.w3.org/2000/svg'%3E%3Cpath d='M20 5c0 0-5 5-5 10s5 10 5 10 5-5 5-10-5-10-5-10zm0 2c2 2 3 6 3 8s-1 6-3 8-3-6-3-8 1-6 3-8z' fill='%23059669' fill-opacity='0.04'/%3E%3C/svg%3E\")",
        }}
      >
        <div className="flex w-full max-w-xl flex-col overflow-hidden rounded-md border border-border bg-card shadow-[0_20px_40px_rgba(0,0,0,0.05)]">
          <div className="grid grid-cols-[80px_1fr_80px] items-center border-b border-border bg-muted px-4 py-3">
            <div className="flex gap-1.5">
              <span className="inline-block h-2.5 w-2.5 rounded-full border border-black/10 bg-[#FF5F56]" />
              <span className="inline-block h-2.5 w-2.5 rounded-full border border-black/10 bg-[#FFBD2E]" />
              <span className="inline-block h-2.5 w-2.5 rounded-full border border-black/10 bg-[#27C93F]" />
            </div>
            <div className="text-center text-[11px] text-muted-foreground">
              extract_receipt.baml
            </div>
            <div />
          </div>
          <div className="flex bg-card">
            <div className="border-r border-border bg-muted px-3 py-4 text-right text-[11px] text-muted-foreground/80 leading-[1.6]">
              {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map((n) => (
                <div key={n}>{n}</div>
              ))}
            </div>
            <div className="w-full overflow-x-auto px-4 py-4 text-[13px] leading-relaxed">
              <pre className="m-0 whitespace-pre">
                <code>
                  <span className="text-[#8B5CF6] font-medium">class</span>{' '}
                  <span className="text-[#059669]">Receipt</span> {'{'}
                  {'\n'}  total <span className="text-[#059669]">float</span>{' '}
                  @description(
                  <span className="text-[#B45309]">
                    "Final amount paid"
                  </span>
                  )
                  {'\n'}  items <span className="text-[#059669]">Item[]</span>
                  {'\n'}  date{'  '}
                  <span className="text-[#059669]">string</span> @description(
                  <span className="text-[#B45309]">"YYYY-MM-DD"</span>)
                  {'\n'}
                  {'}'}
                  {'\n'}
                  {'\n'}
                  <span className="text-[#8B5CF6] font-medium">function</span>{' '}
                  <span className="text-[#0D9488]">ExtractReceipt</span>(img:{' '}
                  <span className="text-[#059669]">Image</span>) -&gt;{' '}
                  <span className="text-[#059669]">Receipt</span> {'{'}
                  {'\n'}  <span className="text-[#8B5CF6] font-medium">client</span>{' '}
                  GPT4o
                  {'\n'}  <span className="text-[#8B5CF6] font-medium">prompt</span>{' '}
                  #"
                  {'\n'}    Extract the items and total from this receipt.
                  {'\n'}    {'{{ ctx.output_format }}'}
                  {'\n'}
                  {'\n'}    Receipt: {'{{ img }}'}
                  {'\n'}  "#
                  {'\n'}
                  {'}'}
                </code>
              </pre>
            </div>
          </div>
        </div>
      </div>
    </section>
  );
}
