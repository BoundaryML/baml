"use client";
import Image from 'next/image';
import Link from 'next/link';
import { Card, CardContent, CardFooter, CardHeader, CardTitle, CardDescription } from '@baml/ui/card';
import { Loader as UiLoader } from '@baml/ui/custom/loader';

export function BrandedLoading({ title = 'BAML Playground' }: { title?: string }) {
  return (
    <div className="relative flex min-h-screen w-full items-center justify-center bg-background">
      <div className="w-full max-w-md">
        <Card className="rounded-2xl border-border/60 bg-card shadow-sm">
          <CardHeader className="gap-3">
            <div className="flex items-center gap-3">
              <Image
                src="/baml-lamb-white.png"
                alt="BAML"
                width={40}
                height={40}
                className="opacity-90"
                priority
              />
              <div className="flex flex-col">
                <CardTitle className="text-2xl leading-none tracking-tight">{title}</CardTitle>
                <CardDescription>Loading your playground</CardDescription>
              </div>
            </div>
          </CardHeader>

          <CardContent>
            <div className="flex items-center justify-center gap-2 py-6 text-base text-muted-foreground">
              <UiLoader />
              <span>Initializing…</span>
            </div>
          </CardContent>

          <CardFooter className="justify-center border-t border-border/60 py-4 text-xs text-muted-foreground">
            Powered by{' '}
            <Link
              href="https://boundaryml.com"
              target="_blank"
              className="ml-1 underline decoration-purple-400/60 underline-offset-4 hover:text-foreground"
            >
              Boundary
            </Link>
          </CardFooter>
        </Card>
      </div>
    </div>
  );
}



