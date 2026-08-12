// import Info from '@components/icons/info'
// import styles from "./mdx-note.module.css";
export function MDXTip({ children, title }: React.HTMLAttributes<HTMLElement>) {
  return (
    <div className="my-6 rounded-r-lg border-l-4 border-blue-500 bg-blue-50 p-4 shadow-md">
      <div className="mb-2 flex items-center">
        <svg
          className="mr-2 h-6 w-6 text-blue-500"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
          xmlns="http://www.w3.org/2000/svg"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z"
          />
        </svg>
        <span className="font-bold text-blue-700">{title ?? 'Tip'}</span>
      </div>
      <div className="text-blue-700">{children}</div>
    </div>
  );
}
