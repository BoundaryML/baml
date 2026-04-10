type ExternalEpisode = {
  folder: string;
  guid: string;
  title: string;
  description: string;
  event_link?: string;
  eventDate: string;
  event_type: string;
  media?: { url: string | null; type?: string };
  links?: { youtube?: string; code?: string; rsvp?: string; discord?: string; connect?: string };
  season?: number;
  episode?: number;
  isPast?: boolean;
  isWorkshop?: boolean;
};

export type PodcastEpisode = {
  date: string;
  description: string;
  episodeNumber: string;
  featured: boolean;
  id: number;
  rsvpUrl?: string;
  title: string;
  topics: string[];
  codeUrl?: string;
  youtubeUrl?: string;
  slug: string;
};

const PODCAST_DATA_URL =
  'https://raw.githubusercontent.com/ai-that-works/ai-that-works/refs/heads/main/data.json';

type FallbackEpisodeInput = Omit<PodcastEpisode, 'slug'> & { slug?: string };

const fallbackEpisodeData: FallbackEpisodeInput[] = [
  {
    date: '2025-07-29',
    description:
      'AI That Works #16 will be a super-practical deep dive into real-world examples and techniques for evaluating a single prompt against multiple models. While this is a commonly heralded use case for Evals, e.g. "how do we know if the new model is better" / "how do we know if the new model breaks anything", there\'s not a ton of practical examples out there for real-world use cases.',
    episodeNumber: '#16',
    featured: true,
    id: 1,
    rsvpUrl: 'https://lu.ma/gnvx0iic',
    title: 'Evaluating Prompts Across Models',
    topics: ['Evaluation', 'Multi-Model', 'Prompts'],
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-07-22-multimodality',
    date: '2025-07-22',
    description:
      "Dive deep into practical PDF processing techniques for AI applications. We'll explore how to extract, parse, and leverage PDF content effectively in your AI workflows, tackling common challenges like layout preservation, table extraction, and multi-modal content handling.",
    episodeNumber: '#15',
    featured: false,
    id: 2,
    title: 'PDFs, Multimodality, Vision Models',
    topics: ['PDFs', 'Multimodality', 'Vision Models'],
    youtubeUrl: 'https://youtu.be/sCScFZB4Am8',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-07-15-decaying-resolution-memory',
    date: '2025-07-15',
    description:
      "Last week on #13, we did a conceptual deep dive on context engineering and memory - this week, we're going to jump right into the weeds and implement a version of Decaying-Resolution Memory that you can pick up and apply to your AI Agents today. For this episode, you'll probably want to check out episode #13 in the session listing to get caught up on DRM and why its worth building from scratch.",
    episodeNumber: '#14',
    featured: false,
    id: 3,
    title: 'Implementing Decaying-Resolution Memory',
    topics: ['Memory', 'AI Agents', 'Implementation'],
    youtubeUrl: 'https://www.youtube.com/watch?v=CEGSDlCtI8U',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-07-08-context-engineering',
    date: '2025-07-08',
    description:
      "How do we build agents that can remember past conversations and learn over time? We'll explore memory and context engineering techniques to create AI systems that maintain state across interactions.",
    episodeNumber: '#13',
    featured: false,
    id: 4,
    title: 'Building AI with Memory & Context',
    topics: ['Memory', 'Context Engineering', 'AI Agents'],
    youtubeUrl: 'https://www.youtube.com/watch?v=-doV02eh8XI',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-07-01-ai-content-pipeline-2',
    date: '2025-07-01',
    description:
      'This week\'s session was a bit meta! We explored "Boosting AI Output Quality" by building the very AI pipeline that generated this email from our Zoom recording. The real breakthrough: separating extraction from polishing for high-quality AI generation.',
    episodeNumber: '#12',
    featured: false,
    id: 5,
    title: 'Boosting AI Output Quality',
    topics: ['Content Generation', 'Quality', 'Pipeline'],
    youtubeUrl: 'https://www.youtube.com/watch?v=HsElHU44xJ0',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-06-24-ai-content-pipeline',
    date: '2025-06-24',
    description:
      "Content creation involves a lot of manual work - uploading videos, sending emails, and other follow-up tasks that are easy to drop. We'll build an agent that integrates YouTube, email, GitHub and human-in-the-loop to fully automate the AI that Works content pipeline, handling all the repetitive work while maintaining quality.",
    episodeNumber: '#11',
    featured: false,
    id: 6,
    title: 'Building an AI Content Pipeline',
    topics: ['Automation', 'Content Pipeline', 'Integration'],
    youtubeUrl: 'https://www.youtube.com/watch?v=Xece-W7Xf48',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-06-17-entity-extraction',
    date: '2025-06-17',
    description:
      "Disambiguating many ways of naming the same thing (companies, skills, etc.) - from entity extraction to resolution to deduping. We'll explore breaking problems into extraction → resolution → enrichment stages, scaling with two-stage designs, and building async workflows with human-in-loop patterns for production entity resolution systems.",
    episodeNumber: '#10',
    featured: false,
    id: 7,
    title: 'Entity Resolution: Extraction, Deduping, and Enriching',
    topics: ['Entity Resolution', 'Data Processing', 'Production Systems'],
    youtubeUrl: 'https://youtu.be/niR896pQWOQ',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-06-10-cracking-the-prompting-interview',
    date: '2025-06-10',
    description:
      "Ready to level up your prompting skills? Join us for a deep dive into advanced prompting techniques that separate good prompt engineers from great ones. We'll dissect real interview scenarios, build prompts live, and show you how to evaluate and iterate like a pro.",
    episodeNumber: '#9',
    featured: false,
    id: 8,
    title: 'Cracking the Prompting Interview',
    topics: ['Prompt Engineering', 'Interviews', 'Best Practices'],
    youtubeUrl: 'https://youtu.be/x5w8Ixntdqs',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-06-03-agent-evals',
    date: '2025-06-03',
    description:
      'This session will be foundational on how to build and scale AI evals. Without good evals, agents fall apart over time and never reach production quality. We will run the evals on both small and large models to compare cost and performance.',
    episodeNumber: '#8',
    featured: false,
    id: 9,
    title: 'Agent Evals: Testing and Scaling',
    topics: ['Agent Evals', 'Testing', 'Scaling'],
    youtubeUrl: 'https://youtu.be/Yi8mYFYYGvs',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-05-28-mcp-tools',
    date: '2025-05-28',
    description:
      "MCP is only as great as your ability to pick the right tools. We'll dive into showing how to leverage MCP servers and accurately use the right ones when only a few have actually relevant tools.",
    episodeNumber: '#7',
    featured: false,
    id: 10,
    title: '12-factor agents: selecting from thousands of MCP tools',
    topics: ['MCP', 'Tool Selection', '12-Factor'],
    youtubeUrl: 'https://www.youtube.com/watch?v=P5wRLKF4bt8',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-05-20-policies-to-prompts',
    date: '2025-05-20',
    description:
      "One of the most common problems in AI engineering is looking at a set of policies / rules and evaluating evidence to determine if the rules were followed. In this session we'll explore turning policies into prompts and pipelines to evaluate which emails in the massive enron email dataset violated SEC and Sarbanes-Oxley regulations.",
    episodeNumber: '#6',
    featured: false,
    id: 11,
    title: 'Policy to Prompt: Evaluating w/ the Enron Emails Dataset',
    topics: ['Policy Evaluation', 'Enron Dataset', 'Compliance'],
    youtubeUrl: 'https://www.youtube.com/watch?v=gkekVC67iVs',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-05-13-designing-evals',
    date: '2025-05-13',
    description:
      'Stay tuned for our season 2 kickoff topic on minimalist and high-performance testing/evals for LLM applications',
    episodeNumber: '#5',
    featured: false,
    id: 12,
    title: 'evals evals evals',
    topics: ['Evaluation', 'Testing', 'LLM Applications'],
    youtubeUrl: 'https://youtu.be/-N6MajRfqYw',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-04-22-twelve-factor-agents',
    date: '2025-04-22',
    description:
      "Learn how to build production-ready AI agents using the twelve-factor methodology. we'll cover the core concepts and build a real agent from scratch.",
    episodeNumber: '#4',
    featured: false,
    id: 13,
    title: 'twelve factor agents',
    topics: ['12-Factor', 'AI Agents', 'Production'],
    youtubeUrl: 'https://youtu.be/yxJDyQ8v6P0',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-04-15-code-generation-with-small-models',
    date: '2025-04-15',
    description:
      "Large models can do a lot, but so can small models. we'll discuss techniques for how to leverge extremely small models for generating diffs and making changes in complete codebases.",
    episodeNumber: '#3',
    featured: false,
    id: 14,
    title: 'code generation with small models',
    topics: ['Code Generation', 'Small Models', 'Optimization'],
    youtubeUrl: 'https://youtu.be/KJkvYdGEnAY',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-04-07-reasoning-models-vs-prompts',
    date: '2025-04-08',
    description:
      "Models can reason but you can also reason within a prompt. which technique wins out when and why? we'll find out by adding reasoning to a chat bot that generates complex cypher/sql queries.",
    episodeNumber: '#2',
    featured: false,
    id: 15,
    title: 'reasoning models vs reasoning prompts',
    topics: ['Reasoning', 'Models', 'Prompts'],
    youtubeUrl: 'https://youtu.be/D-pcKduKdYM',
  },
  {
    codeUrl:
      'https://github.com/hellovai/ai-that-works/tree/main/2025-03-31-large-scale-classification',
    date: '2025-03-31',
    description:
      'LLMs are great at classification from 5, 10, maybe even 50 categories. but how do we deal with situations when we have over 1000? perhaps its an ever changing list of categories?',
    episodeNumber: '#1',
    featured: false,
    id: 16,
    title: 'large scale classification',
    topics: ['Classification', 'Scale', 'Dynamic Categories'],
    youtubeUrl: 'https://youtu.be/6B7MzraQMZk',
  },
];

const slugifyFallbackEpisode = (episode: FallbackEpisodeInput, index: number) => {
  const base = episode.slug ?? episode.title ?? `episode-${index + 1}`;
  return base
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/(^-|-$)+/g, '')
    .replace(/--+/g, '-')
    || `episode-${episode.id ?? index + 1}`;
};

export const fallbackEpisodes: PodcastEpisode[] = fallbackEpisodeData.map((episode, index) => ({
  ...episode,
  slug: slugifyFallbackEpisode(episode, index),
}));

function mapExternalEpisodes(externalEpisodes: ExternalEpisode[]): PodcastEpisode[] {
  const onlyEpisodes = externalEpisodes.filter((episode) => episode.event_type === 'episode');
  if (onlyEpisodes.length === 0) {
    return fallbackEpisodes;
  }

  const ascendingByDate = [...onlyEpisodes].sort(
    (a, b) => new Date(a.eventDate).getTime() - new Date(b.eventDate).getTime(),
  );
  const guidToAbsoluteNumber = new Map<string, number>(
    ascendingByDate.map((episode, index) => [episode.guid, index + 1]),
  );

  return [...onlyEpisodes]
    .sort((a, b) => new Date(b.eventDate).getTime() - new Date(a.eventDate).getTime())
    .map((episode, idx) => {
      const youtubeFromLinks = episode.links?.youtube;
      const youtubeFromMedia = episode.media?.type?.includes('youtube')
        ? episode.media?.url ?? undefined
        : undefined;

      const youtubeUrl = youtubeFromLinks ?? youtubeFromMedia;
      const cleanedTitle = episode.title
        .replace(/^S\d+E\d+\s*(?:-|–|—|:)\s*/i, '')
        .trim();

      const absoluteEpisode = guidToAbsoluteNumber.get(episode.guid) ?? idx + 1;
      const slug = episode.folder || `episode-${absoluteEpisode}`;

      return {
        id: absoluteEpisode,
        date: episode.eventDate,
        description: episode.description,
        episodeNumber: '#' + absoluteEpisode,
        featured: false,
        rsvpUrl: episode.links?.rsvp ?? episode.event_link,
        title: cleanedTitle,
        topics: [],
        codeUrl: episode.links?.code,
        youtubeUrl,
        slug,
      };
    });
}

export async function fetchPodcastEpisodes(): Promise<PodcastEpisode[]> {
  try {
    const res = await fetch(PODCAST_DATA_URL, { next: { revalidate: 600 } });
    if (!res.ok) {
      console.warn(
        `Podcast feed responded with ${res.status} (${res.statusText}), using fallback data.`,
      );
      return fallbackEpisodes;
    }

    const data = (await res.json()) as { episodes?: ExternalEpisode[] };
    if (!data.episodes) {
      console.warn('Podcast feed response missing episodes array, using fallback data.');
      return fallbackEpisodes;
    }

    return mapExternalEpisodes(data.episodes);
  } catch (error) {
    console.warn('Failed to fetch podcast episodes, using fallback data.', error);
    return fallbackEpisodes;
  }
}
