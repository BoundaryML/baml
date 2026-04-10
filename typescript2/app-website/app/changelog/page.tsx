'use client';

import { useEffect } from 'react';
import { useRouter } from 'next/navigation';

export default function ChangelogPage() {
  const router = useRouter();

  useEffect(() => {
    // Redirect to GitHub releases page
    window.location.href = 'https://github.com/boundaryml/baml/releases';
  }, []);

  return (
    <div className="min-h-screen flex items-center justify-center">
      <div className="text-center">
        <h1 className="text-2xl font-bold mb-4">Redirecting to Changelog...</h1>
        <p className="text-muted-foreground">
          Taking you to our GitHub releases page
        </p>
      </div>
    </div>
  );
}
