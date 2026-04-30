import { formatDistanceToNow } from 'date-fns';
import { ArrowRight } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import type { Post } from '../_lib/get-posts';
import { formatCategoryForDisplay } from './category-filter';

const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';
const CARD_BG = '#FBF8F1';

interface BlogPostsGridProps {
  posts: Post[];
}

export function BlogPostsGrid({ posts }: BlogPostsGridProps) {
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns:
          'repeat(auto-fill, minmax(min(100%, 320px), 1fr))',
        gap: 24,
      }}
    >
      {posts.map((post) => {
        const postImage = post.og?.image || post.firstImage;

        return (
          <Link
            href={`/blog/${post.slug}`}
            key={post.slug}
            style={{
              textDecoration: 'none',
              color: 'inherit',
              display: 'block',
            }}
            className="blog-card"
          >
            <article
              style={{
                display: 'flex',
                flexDirection: 'column',
                height: '100%',
                border: `1px solid ${BORDER}`,
                borderRadius: 8,
                background: '#ffffff',
                overflow: 'hidden',
                transition: 'box-shadow 200ms ease, transform 200ms ease',
              }}
              className="blog-card-article"
            >
              <div
                style={{
                  position: 'relative',
                  aspectRatio: '16 / 10',
                  background: postImage
                    ? CARD_BG
                    : 'linear-gradient(135deg, #4C1D95 0%, #6D28D9 45%, #8B5CF6 100%)',
                  borderBottom: `1px solid ${BORDER}`,
                  overflow: 'hidden',
                }}
              >
                {postImage ? (
                  <Image
                    alt={post.title}
                    className="object-cover"
                    fill
                    sizes="(max-width: 768px) 100vw, (max-width: 1024px) 50vw, 33vw"
                    src={postImage}
                    style={{ objectFit: 'cover' }}
                  />
                ) : (
                  <div
                    aria-hidden
                    style={{
                      position: 'absolute',
                      inset: 0,
                      display: 'flex',
                      alignItems: 'center',
                      justifyContent: 'center',
                      color: 'rgba(255,255,255,0.85)',
                      fontFamily: 'var(--font-serif)',
                      fontStyle: 'italic',
                      fontSize: 'clamp(2rem, 4vw, 3rem)',
                      letterSpacing: '-0.02em',
                    }}
                  >
                    BAML
                  </div>
                )}
              </div>

              <div
                style={{
                  padding: '24px 24px 20px',
                  display: 'flex',
                  flexDirection: 'column',
                  flex: 1,
                  gap: 16,
                }}
              >
                <div
                  style={{
                    display: 'flex',
                    alignItems: 'center',
                    gap: 10,
                  }}
                >
                  <span
                    style={{
                      fontSize: 11,
                      fontWeight: 500,
                      letterSpacing: '0.14em',
                      textTransform: 'uppercase',
                      color: ACCENT,
                    }}
                  >
                    {formatCategoryForDisplay(post.tags[0])}
                  </span>
                  <span
                    style={{
                      width: 3,
                      height: 3,
                      borderRadius: '50%',
                      background: BORDER,
                    }}
                  />
                  <span
                    style={{
                      fontSize: 11,
                      letterSpacing: '0.04em',
                      color: EYEBROW,
                    }}
                  >
                    {post.readingTime}
                  </span>
                </div>

                <h3
                  style={{
                    fontSize: 19,
                    fontWeight: 600,
                    lineHeight: 1.25,
                    letterSpacing: '-0.015em',
                    color: INK,
                    margin: 0,
                    display: '-webkit-box',
                    WebkitLineClamp: 2,
                    WebkitBoxOrient: 'vertical',
                    overflow: 'hidden',
                  }}
                >
                  {post.title}
                </h3>

                <p
                  style={{
                    fontSize: 14,
                    lineHeight: 1.55,
                    color: MUTED,
                    margin: 0,
                    display: '-webkit-box',
                    WebkitLineClamp: 3,
                    WebkitBoxOrient: 'vertical',
                    overflow: 'hidden',
                  }}
                >
                  {post.description}
                </p>

                <div
                  style={{
                    marginTop: 'auto',
                    paddingTop: 16,
                    borderTop: `1px solid ${BORDER}`,
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'space-between',
                  }}
                >
                  <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                    {post.author?.imageUrl && (
                      <div
                        style={{
                          position: 'relative',
                          width: 28,
                          height: 28,
                          borderRadius: '50%',
                          overflow: 'hidden',
                          border: `1px solid ${BORDER}`,
                        }}
                      >
                        <Image
                          alt={post.author.name}
                          className="object-cover"
                          fill
                          sizes="28px"
                          src={post.author.imageUrl}
                          style={{ objectFit: 'cover' }}
                        />
                      </div>
                    )}
                    <div style={{ display: 'flex', flexDirection: 'column' }}>
                      <span
                        style={{
                          fontSize: 12,
                          fontWeight: 500,
                          color: INK,
                        }}
                      >
                        {post.author?.name}
                      </span>
                      <span style={{ fontSize: 11, color: EYEBROW }}>
                        {formatDistanceToNow(new Date(post.date), {
                          addSuffix: true,
                        })}
                      </span>
                    </div>
                  </div>
                  <span
                    style={{
                      display: 'inline-flex',
                      alignItems: 'center',
                      gap: 4,
                      fontSize: 12,
                      fontWeight: 500,
                      color: INK,
                    }}
                    className="blog-card-cta"
                  >
                    Read
                    <ArrowRight size={12} />
                  </span>
                </div>
              </div>
            </article>
          </Link>
        );
      })}

      <style>{`
        .blog-card:hover .blog-card-article {
          box-shadow: 0 16px 40px -24px rgba(0,0,0,0.18);
          transform: translateY(-2px);
        }
        .blog-card:hover .blog-card-cta { color: ${ACCENT}; }
      `}</style>
    </div>
  );
}
