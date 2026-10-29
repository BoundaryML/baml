// A callout in a blog post, on the Boundary site's panel (the case-study and enterprise blocks' green
// surface) with an accent rule, the same treatment as a pull quote.
export function MDXTip({ children, title }: React.HTMLAttributes<HTMLElement>) {
  return (
    <aside className="not-prose my-8 rounded-r-xl border-l-2 border-accent bg-panel px-5 py-4 md:px-6 md:py-5">
      <p className="mb-1.5 font-display text-[17px] font-medium leading-6 tracking-[-0.03em] text-ink">
        {title ?? 'Tip'}
      </p>
      <div className="text-[15px] leading-6 text-ink-2 [&_a]:underline [&_a]:underline-offset-[3px] [&_a:hover]:text-accent [&_code]:rounded [&_code]:bg-ink/6 [&_code]:px-1 [&_code]:font-mono [&_code]:text-[0.9em] [&_p]:my-2">
        {children}
      </div>
    </aside>
  );
}
