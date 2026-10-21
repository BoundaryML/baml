import { ArrowRight } from 'lucide-react';
import Link from 'next/link';

export function DocsCard({
  description,
  href,
  title,
}: {
  description: string;
  href: string;
  title: string;
}) {
  return (
    <Link className="docs-card group" href={href}>
      <span className="flex items-center justify-between gap-4 font-semibold">
        {title}
        <ArrowRight
          aria-hidden="true"
          className="size-4 text-muted-foreground transition-transform group-hover:translate-x-1"
        />
      </span>
      <p>{description}</p>
    </Link>
  );
}
