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

interface FeaturedPostProps {
  post: Post;
}

export function FeaturedPost({ post }: FeaturedPostProps) {
  return (
    <Link
      href={`/blog/${post.slug}`}
      style={{
        textDecoration: 'none',
        color: 'inherit',
        display: 'block',
      }}
      className="blog-featured-card"
    >
      <article
        style={{
          display: 'grid',
          gridTemplateColumns: 'minmax(0, 1.3fr) minmax(0, 1fr)',
          gap: 0,
          border: `1px solid ${BORDER}`,
          borderRadius: 8,
          overflow: 'hidden',
          background: '#ffffff',
          transition: 'box-shadow 200ms ease, transform 200ms ease',
        }}
        className="blog-featured-grid"
      >
        <div
          style={{
            position: 'relative',
            aspectRatio: '16 / 10',
            background: CARD_BG,
            borderRight: `1px solid ${BORDER}`,
            overflow: 'hidden',
          }}
          className="blog-featured-image"
        >
          <FeaturedPreviewArt title={post.title} tag={post.tags[0]} />
        </div>

        <div
          style={{
            padding: '40px 36px',
            display: 'flex',
            flexDirection: 'column',
            justifyContent: 'space-between',
            gap: 24,
          }}
        >
          <div>
            <div
              style={{
                display: 'flex',
                alignItems: 'center',
                gap: 12,
                marginBottom: 16,
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
                Featured
              </span>
              <span
                style={{
                  width: 4,
                  height: 4,
                  borderRadius: '50%',
                  background: BORDER,
                }}
              />
              <span
                style={{
                  fontSize: 11,
                  fontWeight: 500,
                  letterSpacing: '0.14em',
                  textTransform: 'uppercase',
                  color: EYEBROW,
                }}
              >
                {formatCategoryForDisplay(post.tags[0])}
              </span>
            </div>

            <h2
              style={{
                fontSize: 'clamp(1.5rem, 2.5vw, 2.1rem)',
                fontWeight: 600,
                lineHeight: 1.15,
                letterSpacing: '-0.02em',
                margin: 0,
                color: INK,
              }}
            >
              {post.title}
            </h2>

            <p
              style={{
                fontSize: 16,
                lineHeight: 1.6,
                color: MUTED,
                margin: '16px 0 0',
                display: '-webkit-box',
                WebkitLineClamp: 3,
                WebkitBoxOrient: 'vertical',
                overflow: 'hidden',
              }}
            >
              {post.description}
            </p>
          </div>

          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              paddingTop: 20,
              borderTop: `1px solid ${BORDER}`,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
              {post.author?.imageUrl && (
                <div
                  style={{
                    position: 'relative',
                    width: 36,
                    height: 36,
                    borderRadius: '50%',
                    overflow: 'hidden',
                    border: `1px solid ${BORDER}`,
                  }}
                >
                  <Image
                    alt={post.author.name}
                    className="object-cover"
                    fill
                    sizes="36px"
                    src={post.author.imageUrl}
                    style={{ objectFit: 'cover' }}
                  />
                </div>
              )}
              <div style={{ display: 'flex', flexDirection: 'column' }}>
                <span
                  style={{
                    fontSize: 13,
                    fontWeight: 500,
                    color: INK,
                  }}
                >
                  {post.author?.name}
                </span>
                <span
                  style={{
                    fontSize: 12,
                    color: EYEBROW,
                  }}
                >
                  {formatDistanceToNow(new Date(post.date), {
                    addSuffix: true,
                  })}
                  {' · '}
                  {post.readingTime}
                </span>
              </div>
            </div>
            <span
              style={{
                display: 'inline-flex',
                alignItems: 'center',
                gap: 6,
                fontSize: 13,
                fontWeight: 500,
                color: INK,
              }}
              className="blog-featured-cta"
            >
              Read
              <ArrowRight size={14} />
            </span>
          </div>
        </div>
      </article>

      <style>{`
        .blog-featured-card:hover article {
          box-shadow: 0 18px 48px -24px rgba(0,0,0,0.18);
          transform: translateY(-2px);
        }
        .blog-featured-card:hover .blog-featured-cta { color: ${ACCENT}; }
        @media (max-width: 900px) {
          .blog-featured-grid { grid-template-columns: 1fr !important; }
          .blog-featured-image { border-right: none !important; border-bottom: 1px solid ${BORDER}; }
        }
      `}</style>
    </Link>
  );
}

function FeaturedPreviewArt({ title, tag }: { title: string; tag?: string }) {
  const category = formatCategoryForDisplay(tag ?? 'blog');

  return (
    <div
      aria-hidden
      style={{
        background:
          'radial-gradient(circle at 82% 18%, rgba(109,40,217,0.16), transparent 30%), linear-gradient(180deg, #FFFDF6 0%, #F4EEE2 100%)',
        inset: 0,
        padding: 32,
        position: 'absolute',
      }}
    >
      <div
        style={{
          border: `1px solid ${BORDER}`,
          borderRadius: 6,
          height: '100%',
          overflow: 'hidden',
          background: '#FFFDF6',
        }}
      >
        <div
          style={{
            background: '#F4EEE2',
            borderBottom: `1px solid ${BORDER}`,
            color: EYEBROW,
            fontSize: 11,
            letterSpacing: '0.12em',
            padding: '9px 12px',
            textTransform: 'uppercase',
          }}
        >
          {category}
        </div>
        <div
          style={{
            color: INK,
            fontSize: 'clamp(1.6rem, 3vw, 2.4rem)',
            fontWeight: 500,
            letterSpacing: '-0.025em',
            lineHeight: 1.08,
            padding: 18,
          }}
        >
          {title}
        </div>
        <div
          style={{
            background: ACCENT,
            height: 2,
            margin: '8px 18px 0',
            opacity: 0.55,
            width: 96,
          }}
        />
      </div>
    </div>
  );
}
