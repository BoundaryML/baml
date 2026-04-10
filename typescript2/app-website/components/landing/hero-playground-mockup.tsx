'use client';

import { useInView } from 'motion/react';
import { useRef } from 'react';
import { BorderBeam } from '@/components/magicui/border-beam';
import { cn } from '@/lib/utils';

export function HeroPlaygroundMockup() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { margin: '-50px', once: true });

  return (
    <div
      className="relative w-full animate-fade-up opacity-0 [--animation-delay:400ms] [perspective:2000px]"
      ref={ref}
    >
      <div
        className={cn(
          'relative overflow-hidden rounded-xl border border-border bg-card shadow-2xl',
          'before:absolute before:bottom-1/2 before:left-0 before:top-0 before:h-full before:w-full before:opacity-0 before:[filter:blur(180px)] before:[background-image:linear-gradient(to_bottom,var(--secondary),var(--secondary),transparent_50%)]',
          inView && 'before:animate-image-glow',
        )}
      >
        <BorderBeam
          colorFrom="var(--secondary)"
          colorTo="var(--primary)"
          delay={11}
          duration={12}
          size={260}
        />

        <div className="relative z-10 flex min-h-[60vh] w-full flex-col sm:min-h-[65vh] lg:min-h-[72vh]">
          {/* Title bar */}
          <div className="flex items-center justify-between border-b border-border bg-muted/50 px-3 py-2.5 text-left">
            <div className="flex items-center gap-2">
              <span className="h-2.5 w-2.5 rounded-full bg-red-500/80" />
              <span className="h-2.5 w-2.5 rounded-full bg-amber-500/80" />
              <span className="h-2.5 w-2.5 rounded-full bg-violet-500/80" />
              <span className="ml-2 text-xs font-medium text-muted-foreground">
                BAML Playground
              </span>
            </div>
            <span className="rounded-full border border-primary/60 bg-primary/10 px-2 py-0.5 text-[10px] font-medium text-primary">
              Turing complete
            </span>
          </div>

          {/* Main: editor + output split */}
          <div className="flex flex-1 min-h-0">
            {/* Left: file tree + editor */}
            <div className="flex min-w-0 flex-1 flex-col border-r border-border">
              {/* File tabs */}
              <div className="flex border-b border-border bg-muted/30 text-left">
                <div className="border-r border-border bg-background px-3 py-2">
                  <span className="text-xs font-medium text-foreground">
                    agent.baml
                  </span>
                </div>
                <button
                  type="button"
                  className="px-3 py-2 text-xs text-muted-foreground hover:text-foreground"
                >
                  clients.baml
                </button>
              </div>

              {/* Editor */}
              <div className="flex flex-1 min-h-0 overflow-auto">
                <div className="w-8 shrink-0 border-r border-border bg-muted/20 py-2 text-right text-[10px] leading-relaxed text-muted-foreground">
                  {[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24].map(
                    (n) => (
                      <div key={n} className="pr-1.5">
                        {n}
                      </div>
                    ),
                  )}
                </div>
                <div className="min-w-0 flex-1 font-mono text-[11px] leading-relaxed text-left text-foreground sm:text-xs">
                  <pre className="whitespace-pre-wrap p-3 text-left">
                    <span className="text-muted-foreground">// Agent: plan then execute steps</span>
                    {'\n\n'}
                    <CodeLine type="keyword">class</CodeLine>{' '}
                    <CodeLine type="type">Plan</CodeLine>{' '}
                    <CodeLine type="bracket">{`{`}</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">goal</CodeLine>{' '}
                    <CodeLine type="keyword">string</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">steps</CodeLine>{' '}
                    <CodeLine type="type">Step</CodeLine>
                    <CodeLine type="bracket">{`[]`}</CodeLine>
                    {'\n'}
                    <CodeLine type="bracket">{`}`}</CodeLine>
                    {'\n\n'}
                    <CodeLine type="keyword">class</CodeLine>{' '}
                    <CodeLine type="type">Step</CodeLine>{' '}
                    <CodeLine type="bracket">{`{`}</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">action</CodeLine>{' '}
                    <CodeLine type="keyword">string</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">result</CodeLine>{' '}
                    <CodeLine type="keyword">string</CodeLine>
                    {'\n'}
                    <CodeLine type="bracket">{`}`}</CodeLine>
                    {'\n\n'}
                    <CodeLine type="keyword">function</CodeLine>{' '}
                    <CodeLine type="function">PlanTask</CodeLine>
                    <CodeLine type="bracket">{`(goal: `}</CodeLine>
                    <CodeLine type="keyword">string</CodeLine>
                    <CodeLine type="bracket">{`) -> `}</CodeLine>
                    <CodeLine type="type">Plan</CodeLine>{' '}
                    <CodeLine type="bracket">{`{`}</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">client</CodeLine>{' '}
                    <CodeLine type="string">openai/gpt-4o</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">retry_policy</CodeLine>{' '}
                    <CodeLine type="keyword">Exponential</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">prompt</CodeLine>{' '}
                    <CodeLine type="string">#"Break down: </CodeLine><CodeLine type="template">{'{{ goal }}'}</CodeLine><CodeLine type="string">"#</CodeLine>
                    {'\n'}
                    <CodeLine type="bracket">{`}`}</CodeLine>
                    {'\n\n'}
                    <CodeLine type="keyword">function</CodeLine>{' '}
                    <CodeLine type="function">ExecuteStep</CodeLine>
                    <CodeLine type="bracket">{`(step: `}</CodeLine>
                    <CodeLine type="type">Step</CodeLine>
                    <CodeLine type="bracket">{`) -> `}</CodeLine>
                    <CodeLine type="keyword">string</CodeLine>{' '}
                    <CodeLine type="bracket">{`{`}</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">client</CodeLine>{' '}
                    <CodeLine type="string">anthropic/claude-3-5-sonnet</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">fallback</CodeLine>{' '}
                    <CodeLine type="string">openai/gpt-4o</CodeLine>
                    {'\n  '}
                    <CodeLine type="property">prompt</CodeLine>{' '}
                    <CodeLine type="string">#"Execute: </CodeLine><CodeLine type="template">{'{{ step.action }}'}</CodeLine><CodeLine type="string">"#</CodeLine>
                    {'\n'}
                    <CodeLine type="bracket">{`}`}</CodeLine>
                  </pre>
                </div>
              </div>
            </div>

            {/* Right: output panel */}
            <div className="flex w-[42%] min-w-0 flex-col bg-muted/20 lg:w-[38%]">
              <div className="flex items-center justify-between border-b border-border bg-muted/40 px-3 py-1.5">
                <span className="text-[10px] font-medium text-muted-foreground">
                  Output
                </span>
                <span className="text-[10px] text-violet-600 dark:text-violet-300">
                  ✓ Run completed
                </span>
              </div>
              <div className="flex-1 overflow-auto p-3">
                <pre className="font-mono text-[10px] leading-relaxed text-foreground/90 sm:text-[11px]">
                  <span className="text-muted-foreground">{`{`}</span>
                  {'\n  '}
                  <span className="text-violet-600 dark:text-violet-300">"goal"</span>
                  <span className="text-muted-foreground">: </span>
                  <span className="text-violet-500 dark:text-violet-200">"Deploy API"</span>
                  <span className="text-muted-foreground">,</span>
                  {'\n  '}
                  <span className="text-violet-600 dark:text-violet-300">"steps"</span>
                  <span className="text-muted-foreground">: [</span>
                  {'\n    '}
                  <span className="text-muted-foreground">{`{ "action": "Write tests", "result": "..." },`}</span>
                  {'\n    '}
                  <span className="text-muted-foreground">{`{ "action": "Deploy to Vercel", "result": "..." }`}</span>
                  {'\n  '}
                  <span className="text-muted-foreground">]</span>
                  {'\n'}
                  <span className="text-muted-foreground">{`}`}</span>
                </pre>
              </div>
            </div>
          </div>

          {/* Status bar */}
          <div className="flex items-center justify-between border-t border-border bg-muted/40 px-3 py-1.5 text-left">
            <span className="text-[10px] text-muted-foreground">
              agent.baml · L24 · Run with Cmd+Enter
            </span>
            <span className="text-[10px] text-muted-foreground">
              BAML
            </span>
          </div>
        </div>
      </div>
    </div>
  );
}

function CodeLine({
  type,
  children,
}: {
  type:
    | 'keyword'
    | 'type'
    | 'function'
    | 'property'
    | 'string'
    | 'bracket'
    | 'template';
  children: React.ReactNode;
}) {
  const className = {
    keyword: 'text-blue-600 dark:text-blue-400',
    type: 'text-amber-600 dark:text-amber-400',
    function: 'text-violet-600 dark:text-violet-400',
    property: 'text-emerald-600 dark:text-emerald-400',
    string: 'text-green-600 dark:text-green-400',
    bracket: 'text-foreground/90',
    template: 'text-orange-600 dark:text-orange-400',
  }[type];
  return <span className={className}>{children}</span>;
}
