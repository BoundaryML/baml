export function MDXNote({
  children,
  ...props
}: React.HTMLAttributes<HTMLElement>) {
  return (
    <aside
      {...props}
      className="my-6 rounded-lg border border-blue-200 bg-blue-50 p-4 dark:border-blue-800 dark:bg-blue-950/30"
    >
      <div className="flex gap-3">
        <div className="flex-shrink-0 text-blue-600 dark:text-blue-400">
          <svg
            className="h-5 w-5"
            fill="currentColor"
            viewBox="0 0 20 20"
            xmlns="http://www.w3.org/2000/svg"
          >
            <path
              fillRule="evenodd"
              d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z"
              clipRule="evenodd"
            />
          </svg>
        </div>
        <div className="flex-1 text-sm text-blue-800 dark:text-blue-200">
          <span className="font-semibold">Note: </span>
          {children}
        </div>
      </div>
    </aside>
  );
}
