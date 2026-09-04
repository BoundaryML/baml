'use client';

import { Github } from 'lucide-react';
import { useEffect, useState } from 'react';
import { z } from 'zod';

const repositoryApiUrl = 'https://api.github.com/repos/BoundaryML/baml';
const repositoryUrl = 'https://github.com/BoundaryML/baml';
const repositoryResponseSchema = z
  .object({
    stargazers_count: z.number().int().nonnegative(),
  })
  .passthrough();

function formatStars(count: number) {
  return count >= 1000
    ? `${Math.round(count / 1000)}k`
    : count.toLocaleString();
}

export function GitHubLink() {
  const [stars, setStars] = useState<number | null>(null);

  useEffect(() => {
    const controller = new AbortController();

    async function loadStars() {
      try {
        const response = await fetch(repositoryApiUrl, {
          headers: { Accept: 'application/vnd.github+json' },
          signal: controller.signal,
        });
        if (!response.ok) return;

        const parsedResponse = repositoryResponseSchema.safeParse(
          await response.json(),
        );
        if (parsedResponse.success)
          setStars(parsedResponse.data.stargazers_count);
      } catch (error) {
        if (!(error instanceof DOMException && error.name === 'AbortError')) {
          console.error('Unable to load the GitHub star count.', error);
        }
      }
    }

    void loadStars();
    return () => controller.abort();
  }, []);

  return (
    <a
      aria-label={
        stars === null
          ? 'BAML on GitHub'
          : `BAML on GitHub, ${stars.toLocaleString()} stars`
      }
      className="docs-focus-ring inline-flex h-8 items-center gap-1.5 rounded-md px-2.5 text-sm font-medium hover:bg-accent"
      href={repositoryUrl}
      rel="noreferrer"
      target="_blank"
    >
      <Github aria-hidden="true" className="size-4" />
      {stars === null ? (
        <span
          aria-hidden="true"
          className="h-4 w-[42px] animate-pulse rounded-sm bg-muted"
        />
      ) : (
        <span
          className="w-fit text-xs text-muted-foreground tabular-nums"
          title={`${stars.toLocaleString()} stars`}
        >
          {formatStars(stars)}
        </span>
      )}
    </a>
  );
}
