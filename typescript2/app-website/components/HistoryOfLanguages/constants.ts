import type { ShapeId } from './shapes';

export type TimelinePanelDef = {
  id: string;
  language: string;
  year: string;
  transitionText: string;
  tagline: string;
  shapeId: ShapeId;
  isBaml?: boolean;
};

export const BAML_SNIPPET = `function ClassifyMessage(message: string) -> Category {
  client "openai/gpt-4o"
  prompt #"
    Classify this message into a category:

    {{ message }}

    {{ ctx.output_format }}
  "#
}

enum Category {
  Greeting
  Question
  Complaint
  Other
}`;

export const HISTORY_LANGUAGE_PANELS: TimelinePanelDef[] = [
  {
    id: 'assembly',
    language: 'Assembly',
    year: '1949',
    transitionText: 'In the beginning, we spoke to machines in their language...',
    tagline: 'Closest to the machine',
    shapeId: 'sphinx',
  },
  {
    id: 'c',
    language: 'C',
    year: '1972',
    transitionText: 'Then we built an empire...',
    tagline: 'The empire everything was built on',
    shapeId: 'colosseum',
  },
  {
    id: 'cpp',
    language: 'C++',
    year: '1985',
    transitionText: 'And reached for the sky...',
    tagline: "Towering complexity on C's foundation",
    shapeId: 'cathedral',
  },
  {
    id: 'python',
    language: 'Python',
    year: '1991',
    transitionText: 'Until we realized everyone should build...',
    tagline: 'Made programming accessible to everyone',
    shapeId: 'goldengate',
  },
  {
    id: 'rust',
    language: 'Rust',
    year: '2015',
    transitionText: 'So we made it safe to build anything...',
    tagline: 'Safety and speed at extreme scale',
    shapeId: 'burjkhalifa',
  },
  {
    id: 'baml',
    language: 'BAML',
    year: '2024',
    transitionText: 'Now, we teach machines to speak ours.',
    tagline: 'The language for the AI era',
    shapeId: 'spaceelevator',
    isBaml: true,
  },
];
