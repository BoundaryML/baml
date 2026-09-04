'use client';

import { usePathname, useRouter } from 'next/navigation';

import type { GeneratedVersionOption } from '@/lib/generated-content/discovery';

export function GeneratedVersionSwitcher({
  options,
}: {
  options: GeneratedVersionOption[];
}) {
  const pathname = usePathname();
  const router = useRouter();
  const current = options.find((option) => option.href === pathname);

  if (options.length === 0) return null;

  return (
    <label className="flex max-w-md items-center gap-3 rounded-lg border bg-muted/40 px-3 py-2 text-sm">
      <span className="font-medium">Reference version</span>
      <select
        aria-label="Reference version"
        className="ml-auto min-w-0 rounded border border-input bg-background px-2 py-1 font-mono text-xs text-foreground"
        onChange={(event) => router.push(event.target.value)}
        value={current?.href ?? options[0]?.href}
      >
        {options.map((option) => (
          <option key={option.href} value={option.href}>
            {option.routeVersion}
            {option.channels.length ? ` (${option.channels.join(', ')})` : ''}
          </option>
        ))}
      </select>
    </label>
  );
}
