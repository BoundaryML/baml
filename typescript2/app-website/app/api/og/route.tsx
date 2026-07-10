import { ImageResponse } from 'next/og';
import type { NextRequest } from 'next/server';
import { getPost } from '@/app/blog/_lib/get-posts';
import {
  lambDataUri,
  ogFonts,
  podcastHosts,
  teamPhoto,
} from '@/components/og/og-assets';
import { OG_SIZE, OgCard } from '@/components/og/og-card';

export const runtime = 'nodejs';

// Pick a headline size that keeps long titles inside the card.
function titleFontSize(title: string): number {
  const n = title.length;
  if (n > 62) return 54;
  if (n > 42) return 66;
  if (n > 24) return 78;
  return 92;
}

function clamp(text: string, max: number): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max - 1).trimEnd()}…`;
}

export async function GET(request: NextRequest) {
  const params = request.nextUrl.searchParams;
  const slug = params.get('slug');

  let eyebrow = params.get('eyebrow') || '';
  let title = params.get('title') || 'The programming language for agents';
  let description = params.get('desc') || '';
  let footer = params.get('footer') || 'boundaryml.com';
  const timeline = params.get('timeline') === '1';
  const avatars = params.get('podcast') === '1' ? podcastHosts() : undefined;
  const photo = params.get('team') === '1' ? teamPhoto() : undefined;

  // Blog posts derive their card from the post's frontmatter.
  if (slug) {
    const post = await getPost(slug);
    if (post) {
      eyebrow = post.tags?.[0] ? `Blog · ${post.tags[0]}` : 'Blog';
      title = post.title;
      description = post.description || '';
      footer = post.author?.name
        ? `${post.author.name} · boundaryml.com`
        : 'boundaryml.com';
    } else {
      eyebrow = 'Blog';
      title = 'Post not found';
      description = '';
    }
  }

  return new ImageResponse(
    OgCard({
      avatars,
      description: clamp(description, 140),
      eyebrow: clamp(eyebrow, 42),
      footer,
      lamb: lambDataUri(),
      photo,
      timeline,
      title: clamp(title, 90),
      titleFontSize: timeline
        ? Math.min(titleFontSize(title), 60)
        : titleFontSize(title),
    }),
    { ...OG_SIZE, fonts: ogFonts() },
  );
}
