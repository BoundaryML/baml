// A side note in a blog post: the Boundary site's hairline panel, quieter than a Tip.
export function MDXNote({
  children,
  ...props
}: React.HTMLAttributes<HTMLElement>) {
  return (
    <aside
      {...props}
      className="not-prose my-7 rounded-xl border border-hair px-5 py-4 text-[15px] leading-6 text-ink-2 [&_a]:underline [&_a]:underline-offset-[3px] [&_a:hover]:text-accent [&_code]:rounded [&_code]:bg-ink/6 [&_code]:px-1 [&_code]:font-mono [&_code]:text-[0.9em]"
    >
      <span className="font-medium text-ink">Note: </span>
      {children}
    </aside>
  );
}
