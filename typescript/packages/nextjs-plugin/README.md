# @boundaryml/nextjs

Next.js integration for BAML, providing seamless support for server components, server actions, and streaming responses.

## Installation

```bash
npm install @boundaryml/baml @boundaryml/nextjs
```

## Setup

1. Update your Next.js configuration:

```typescript
// next.config.ts
import { withBaml } from '@boundaryml/nextjs';
import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  // ... your existing config
};

export default withBaml()(nextConfig);
```

2. Use BAML functions in your Server Actions:

```typescript
// app/actions/story.ts
'use server'

import { b } from "@/baml_client";

export async function writeMeAStoryAction(prompt: string) {
  return b.WriteMeAStory.stream(prompt).toStreamable();
}
```

3. Use the generated hooks in your React components:

```typescript
// app/components/story.tsx
'use client'

import { useWriteMeAStoryAction } from "@/baml_client/nextjs";

export function StoryForm() {
  const {
    data: finalStory,
    partialData: streamingStory,
    isLoading,
    isError,
    error,
    mutate
  } = useWriteMeAStoryAction();

  const story = isLoading ? streamingStory : finalStory;

  return (
    <div>
      <button onClick={() => mutate("Once upon a time...")}>
        Generate Story
      </button>
      {isLoading && <p>Generating story...</p>}
      {story && (
        <div>
          <h2>{story.title}</h2>
          <p>{story.content}</p>
        </div>
      )}
    </div>
  );
}
```

## Features

- 🔒 Secure: Runs BAML functions on the server to keep API keys safe
- 🌊 Streaming: Built-in support for streaming responses
- 🎯 Type-safe: Full TypeScript support for all BAML functions
- ⚡️ Fast: Optimized for Next.js App Router and React Server Components
- 🛠 Easy: Zero-config setup with `withBaml`

## API Reference

### `withBaml(config?: BamlNextConfig)`

Wraps your Next.js configuration with BAML integration.

```typescript
interface BamlNextConfig {
  webpack?: NextConfig['webpack']; // Custom webpack configuration
}
```

### Generated Hooks

For each BAML function, a corresponding React hook is generated with the following interface:

```typescript
interface BamlStreamHookResult<T> {
  data?: T;              // Final result
  partialData?: T;       // Streaming result
  isLoading: boolean;    // Loading state
  error?: Error;         // Error state
  mutate: (input: any) => Promise<void>; // Function to call the BAML function
}
```

## License

MIT