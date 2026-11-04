import type { Mark } from './marks';

/** A product's mark in its current text color, with its name in a label above it on hover. A product
 *  without a mark is a tile with its initials. `crossed` draws a line through it, for a product BAML
 *  replaces. */
export function MarkTile({
  crossed,
  mark,
  size = 20,
}: {
  crossed?: boolean;
  mark: Mark;
  size?: number;
}) {
  return (
    <span
      aria-label={mark.name}
      className="group/mark relative inline-grid place-items-center shrink-0 transition-opacity duration-500"
      role="img"
      style={{ height: size, opacity: crossed ? 0.35 : 1, width: size }}
    >
      {mark.path ? (
        <svg
          aria-hidden="true"
          fill="currentColor"
          height={size * 0.82}
          viewBox="0 0 24 24"
          width={size * 0.82}
        >
          <path d={mark.path} />
        </svg>
      ) : (
        <span
          aria-hidden
          className="grid place-items-center size-full rounded-[4px] border border-current font-mono font-medium leading-none tracking-[-0.04em]"
          style={{ fontSize: size * 0.42 }}
        >
          {mark.letters}
        </span>
      )}
      {crossed && (
        <span
          aria-hidden
          className="absolute left-[-15%] right-[-15%] top-1/2 h-[1.5px] -rotate-45 bg-current"
        />
      )}
      <span
        aria-hidden
        className="pointer-events-none absolute bottom-full left-1/2 z-10 mb-1.5 -translate-x-1/2 whitespace-nowrap rounded-md bg-black px-2 py-1 font-sans text-[12px] leading-none text-white opacity-0 transition-opacity duration-150 group-hover/mark:opacity-100"
      >
        {mark.name}
      </span>
    </span>
  );
}
