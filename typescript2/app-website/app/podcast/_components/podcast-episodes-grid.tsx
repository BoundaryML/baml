'use client';

import { formatDistanceToNow } from 'date-fns';
import { ArrowRight, Calendar, Code, Play } from 'lucide-react';
import Link from 'next/link';
import posthog from 'posthog-js';

const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';
const CARD_BG = '#FBF8F1';
const MONO =
  '"IBM Plex Mono", ui-monospace, SFMono-Regular, Menlo, Consolas, monospace';

interface PodcastEpisode {
  codeUrl?: string;
  date: string;
  description: string;
  episodeNumber: string;
  featured: boolean;
  id: number;
  rsvpUrl?: string;
  slug: string;
  title: string;
  topics: string[];
  youtubeUrl?: string;
}

interface PodcastEpisodesGridProps {
  episodes: PodcastEpisode[];
}

export function PodcastEpisodesGrid({ episodes }: PodcastEpisodesGridProps) {
  return (
    <div
      style={{
        display: 'grid',
        gap: 24,
        gridTemplateColumns: 'repeat(auto-fill, minmax(min(100%, 320px), 1fr))',
      }}
    >
      {episodes.map((episode) => {
        const isUpcoming = new Date(episode.date) > new Date();

        return (
          <Link
            className="podcast-card"
            href={`/podcast/${episode.slug}`}
            key={episode.id}
            onClick={() =>
              posthog.capture('podcast_episode_clicked', {
                episode_id: episode.id,
                episode_number: episode.episodeNumber,
                episode_title: episode.title,
                is_upcoming: isUpcoming,
              })
            }
            style={{
              color: 'inherit',
              display: 'block',
              textDecoration: 'none',
            }}
          >
            <article
              className="podcast-card-article"
              style={{
                background: '#ffffff',
                border: `1px solid ${BORDER}`,
                borderRadius: 8,
                display: 'flex',
                flexDirection: 'column',
                height: '100%',
                overflow: 'hidden',
                transition: 'box-shadow 200ms ease, transform 200ms ease',
              }}
            >
              <div
                style={{
                  aspectRatio: '16 / 10',
                  background: CARD_BG,
                  borderBottom: `1px solid ${BORDER}`,
                  overflow: 'hidden',
                  position: 'relative',
                }}
              >
                <EpisodePreviewArt episode={episode} isUpcoming={isUpcoming} />
              </div>

              <div
                style={{
                  display: 'flex',
                  flex: 1,
                  flexDirection: 'column',
                  gap: 16,
                  padding: '24px 24px 20px',
                }}
              >
                <div
                  style={{
                    alignItems: 'center',
                    display: 'flex',
                    gap: 10,
                  }}
                >
                  <span
                    style={{
                      color: ACCENT,
                      fontSize: 11,
                      fontWeight: 500,
                      letterSpacing: '0.14em',
                      textTransform: 'uppercase',
                    }}
                  >
                    {episode.episodeNumber}
                  </span>
                  <span
                    style={{
                      background: BORDER,
                      borderRadius: '50%',
                      height: 3,
                      width: 3,
                    }}
                  />
                  <span
                    style={{
                      color: EYEBROW,
                      fontSize: 11,
                      letterSpacing: '0.04em',
                    }}
                  >
                    {formatDistanceToNow(new Date(episode.date), {
                      addSuffix: true,
                    })}
                  </span>
                </div>

                <h3
                  style={{
                    color: INK,
                    display: '-webkit-box',
                    fontSize: 19,
                    fontWeight: 600,
                    letterSpacing: '-0.015em',
                    lineHeight: 1.25,
                    margin: 0,
                    overflow: 'hidden',
                    WebkitBoxOrient: 'vertical',
                    WebkitLineClamp: 2,
                  }}
                >
                  {episode.title}
                </h3>

                <p
                  style={{
                    color: MUTED,
                    display: '-webkit-box',
                    fontSize: 14,
                    lineHeight: 1.55,
                    margin: 0,
                    overflow: 'hidden',
                    WebkitBoxOrient: 'vertical',
                    WebkitLineClamp: 3,
                  }}
                >
                  {episode.description}
                </p>

                <div
                  style={{
                    display: 'flex',
                    flexWrap: 'wrap',
                    gap: 8,
                    marginTop: 'auto',
                  }}
                >
                  {episode.topics.slice(0, 3).map((topic) => (
                    <span
                      key={topic}
                      style={{
                        background: '#F4EEE2',
                        border: `1px solid ${BORDER}`,
                        borderRadius: 999,
                        color: MUTED,
                        fontSize: 11,
                        padding: '4px 8px',
                      }}
                    >
                      {topic}
                    </span>
                  ))}
                </div>

                <div
                  style={{
                    alignItems: 'center',
                    borderTop: `1px solid ${BORDER}`,
                    display: 'flex',
                    justifyContent: 'space-between',
                    paddingTop: 16,
                  }}
                >
                  <div style={{ display: 'flex', gap: 12 }}>
                    {episode.codeUrl && !isUpcoming && (
                      <span
                        style={{
                          alignItems: 'center',
                          color: EYEBROW,
                          display: 'inline-flex',
                          fontSize: 12,
                          gap: 5,
                        }}
                      >
                        <Code size={12} />
                        Code
                      </span>
                    )}
                    {episode.rsvpUrl && !episode.youtubeUrl && (
                      <span
                        style={{
                          alignItems: 'center',
                          color: EYEBROW,
                          display: 'inline-flex',
                          fontSize: 12,
                          gap: 5,
                        }}
                      >
                        <Calendar size={12} />
                        RSVP
                      </span>
                    )}
                  </div>
                  <span
                    className="podcast-card-cta"
                    style={{
                      alignItems: 'center',
                      color: INK,
                      display: 'inline-flex',
                      fontSize: 12,
                      fontWeight: 500,
                      gap: 4,
                    }}
                  >
                    {episode.youtubeUrl
                      ? 'Watch'
                      : isUpcoming
                        ? 'RSVP'
                        : 'View'}
                    <ArrowRight size={12} />
                  </span>
                </div>
              </div>
            </article>
          </Link>
        );
      })}

      <style>{`
        .podcast-card:hover .podcast-card-article {
          box-shadow: 0 16px 40px -24px rgba(0,0,0,0.18);
          transform: translateY(-2px);
        }
        .podcast-card:hover .podcast-card-cta { color: ${ACCENT}; }
      `}</style>
    </div>
  );
}

function EpisodePreviewArt({
  episode,
  isUpcoming,
}: {
  episode: PodcastEpisode;
  isUpcoming: boolean;
}) {
  return (
    <div
      aria-hidden
      style={{
        background:
          'radial-gradient(circle at 78% 22%, rgba(109,40,217,0.16), transparent 28%), linear-gradient(180deg, #FFFDF6 0%, #F4EEE2 100%)',
        inset: 0,
        padding: 20,
        position: 'absolute',
      }}
    >
      <div
        style={{
          alignItems: 'center',
          color: ACCENT,
          display: 'flex',
          fontFamily: MONO,
          fontSize: 11,
          gap: 8,
          letterSpacing: '0.1em',
          textTransform: 'uppercase',
        }}
      >
        <Play size={13} />
        {isUpcoming ? 'Live soon' : 'Session'}
      </div>
      <div
        style={{
          color: INK,
          fontSize: 'clamp(1.25rem, 2.2vw, 1.8rem)',
          fontWeight: 500,
          letterSpacing: '-0.02em',
          lineHeight: 1.15,
          marginTop: 22,
          maxWidth: '88%',
        }}
      >
        {episode.title}
      </div>
      <div
        style={{
          background: ACCENT,
          bottom: 20,
          height: 2,
          left: 20,
          opacity: 0.55,
          position: 'absolute',
          width: 72,
        }}
      />
    </div>
  );
}
