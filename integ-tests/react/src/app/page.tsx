'use client';

import { ExampleSelector } from '@/components/example-selector';
import { TestClient } from '@/components/test-client';
import { ModeToggle } from '@/components/theme-toggle';
import { Button } from '@/components/ui/button';
import { GithubIcon } from 'lucide-react';
import Image from 'next/image';

export default function Home() {
  return (
    <div className="min-h-screen bg-background">
      <div className="container mx-auto px-4 py-8">
        <header className="mb-8 flex items-center justify-between">
          <div className="flex items-center gap-2">
            <Image
              className="dark:invert"
              src="/next.svg"
              alt="Next.js logo"
              width={100}
              height={20}
              priority
            />
            <span className="font-mono text-lg">+</span>
            <span className="font-bold text-lg">BAML</span>
          </div>
          <div className="flex items-center gap-4">
            <Button asChild variant="outline">
              <a
                href="https://docs.boundaryml.com"
                target="_blank"
                rel="noopener noreferrer"
              >
                Documentation
              </a>
            </Button>
            <Button asChild>
              <a
                href="https://docs.boundaryml.com/docs/examples"
                target="_blank"
                rel="noopener noreferrer"
              >
                View Examples
              </a>
            </Button>
            <Button variant="outline" size="icon" asChild>
              <a
                href="https://github.com/boundaryml/baml"
                target="_blank"
                rel="noopener noreferrer"
              >
                <GithubIcon className="h-4 w-4" />
              </a>
            </Button>
            <ModeToggle />
          </div>
        </header>

        <main className="mx-auto w-full max-w-[95vw] space-y-8">
          <ExampleSelector />

          <div className="w-full">
            <TestClient />
          </div>
        </main>

        <footer className="mt-16 text-center">
          <p className="text-muted-foreground text-sm">
            Built with{' '}
            <a
              href="https://nextjs.org"
              target="_blank"
              rel="noopener noreferrer"
              className="font-medium underline underline-offset-4"
            >
              Next.js
            </a>{' '}
            and{' '}
            <a
              href="https://boundaryml.com"
              target="_blank"
              rel="noopener noreferrer"
              className="font-medium underline underline-offset-4"
            >
              BAML
            </a>
          </p>
        </footer>
      </div>
    </div>
  );
}
