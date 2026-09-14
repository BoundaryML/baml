export function BrandMark({ className = 'size-6' }: { className?: string }) {
  return (
    <svg
      aria-hidden="true"
      className={className}
      fill="none"
      viewBox="0 0 32 32"
    >
      <rect fill="currentColor" height="32" rx="8" width="32" />
      <path
        d="M9.5 22.5 15.7 8h2.75l-2.1 5.05 6.15 9.45h-3.15l-4.3-6.8-2.8 6.8H9.5Z"
        fill="white"
      />
    </svg>
  );
}
