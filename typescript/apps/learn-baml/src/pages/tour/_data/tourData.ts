export interface TourModule {
  slug: string;
  title: string;
  shortTitle: string;
  description: string;
  duration: string;
}

export interface TourChapter {
  id: string;
  title: string;
  description: string;
  modules: TourModule[];
}

export const tourChapters: TourChapter[] = [
  {
    id: 'foundations',
    title: 'Foundations',
    description: 'Core concepts in 7 minutes',
    modules: [
      {
        slug: 'hello-baml',
        title: '1. Hello BAML',
        shortTitle: 'Hello BAML',
        description: 'Write an LLM function in 10 lines. See typed outputs, not strings.',
        duration: '2 min',
      },
      {
        slug: 'prompt-transparency',
        title: '2. Prompt Transparency',
        shortTitle: 'Prompt Transparency',
        description: 'See exactly what prompt gets sent to the model. No hidden abstractions.',
        duration: '2 min',
      },
      {
        slug: 'types-at-work',
        title: '3. Types That Work',
        shortTitle: 'Types That Work',
        description: 'Your schema IS your prompt. Define types that drive parsing automatically.',
        duration: '3 min',
      },
    ],
  },
  {
    id: 'type-mastery',
    title: 'Type Mastery',
    description: 'Advanced type features in 9 minutes',
    modules: [
      {
        slug: 'union-types',
        title: '4. Union Types',
        shortTitle: 'Union Types',
        description: 'Handle multiple outcomes with type safety. Success, errors, and clarifications.',
        duration: '3 min',
      },
      {
        slug: 'attributes',
        title: '5. Attributes',
        shortTitle: 'Attributes',
        description: '@description, @alias, @skip—fine-tune how types become prompts.',
        duration: '3 min',
      },
      {
        slug: 'match-expressions',
        title: '6. Match',
        shortTitle: 'Match',
        description: 'Exhaustive handling of union types. The compiler ensures you handle every case.',
        duration: '3 min',
      },
    ],
  },
  {
    id: 'production',
    title: 'Production',
    description: 'Ship with confidence in 8 minutes',
    modules: [
      {
        slug: 'testing-loop',
        title: '7. Testing',
        shortTitle: 'Testing',
        description: 'Prompts deserve tests too. CI/CD for prompts with assertions.',
        duration: '3 min',
      },
      {
        slug: 'streaming',
        title: '8. Streaming',
        shortTitle: 'Streaming',
        description: 'Stream typed data, not just text. Access fields as they arrive.',
        duration: '3 min',
      },
      {
        slug: 'client-strategies',
        title: '9. Reliability',
        shortTitle: 'Reliability',
        description: 'Retry policies and fallback chains. Declarative reliability without code.',
        duration: '2 min',
      },
    ],
  },
  {
    id: 'advanced',
    title: 'Advanced',
    description: 'Power features in 11 minutes',
    modules: [
      {
        slug: 'polyglot-integration',
        title: '10. Integration',
        shortTitle: 'Integration',
        description: 'Call BAML from TypeScript, Python, and more. Types flow through.',
        duration: '3 min',
      },
      {
        slug: 'dynamic-types',
        title: '11. Dynamic Types',
        shortTitle: 'Dynamic Types',
        description: 'Add fields at runtime with TypeBuilder. Schemas that adapt to your needs.',
        duration: '3 min',
      },
      {
        slug: 'putting-it-together',
        title: '12. Real-World',
        shortTitle: 'Real-World',
        description: 'Build a production-ready Customer Support Triage Bot. Everything combined.',
        duration: '5 min',
      },
    ],
  },
];

// Helper functions
export function getAllModules(): TourModule[] {
  return tourChapters.flatMap(chapter => chapter.modules);
}

export function getModuleIndex(slug: string): number {
  const modules = getAllModules();
  return modules.findIndex(m => m.slug === slug);
}

export function getChapterForModule(slug: string): TourChapter | undefined {
  return tourChapters.find(chapter => 
    chapter.modules.some(m => m.slug === slug)
  );
}

export function getModuleBySlug(slug: string): TourModule | undefined {
  return getAllModules().find(m => m.slug === slug);
}

export function getAdjacentModules(slug: string): { prev: TourModule | null; next: TourModule | null } {
  const modules = getAllModules();
  const index = modules.findIndex(m => m.slug === slug);
  return {
    prev: index > 0 ? modules[index - 1] : null,
    next: index < modules.length - 1 ? modules[index + 1] : null,
  };
}

export function getTotalDuration(): string {
  let totalMinutes = 0;
  getAllModules().forEach(m => {
    const match = m.duration.match(/(\d+)/);
    if (match) totalMinutes += parseInt(match[1], 10);
  });
  return `~${totalMinutes} minutes`;
}
