'use client';

import { format } from 'date-fns';
import { ArrowRight, ChevronDown } from 'lucide-react';
import Image from 'next/image';
import Link from 'next/link';
import { useState } from 'react';
import type { Post } from '../_lib/get-posts';
import { formatCategoryForDisplay } from './category-filter';

const INK = '#1A1612';
const MUTED = '#5C5852';
const BORDER = '#D9D3C4';
const ACCENT = '#6D28D9';
const EYEBROW = '#8A8580';

function formatDate(date: string): string {
  const d = new Date(date);
  return Number.isNaN(d.getTime()) ? date : format(d, 'MMM d, yyyy');
}

interface BlogListProps {
  posts: Post[];
}

export function BlogList({ posts }: BlogListProps) {
  const [open, setOpen] = useState<Set<string>>(() => new Set());

  const toggle = (slug: string) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (next.has(slug)) {
        next.delete(slug);
      } else {
        next.add(slug);
      }
      return next;
    });

  return (
    <section style={{ width: '100%', padding: '48px 48px 96px' }}>
      <div style={{ margin: '0 auto', maxWidth: 1080 }}>
        <div
          style={{
            alignItems: 'baseline',
            borderBottom: `1px solid ${BORDER}`,
            display: 'flex',
            gap: 16,
            justifyContent: 'space-between',
            marginBottom: 4,
            paddingBottom: 16,
          }}
        >
          <p
            style={{
              color: EYEBROW,
              fontSize: 12,
              fontWeight: 500,
              letterSpacing: '0.14em',
              margin: 0,
              textTransform: 'uppercase',
            }}
          >
            All posts
          </p>
          <p
            style={{
              color: EYEBROW,
              fontSize: 12,
              letterSpacing: '0.04em',
              margin: 0,
            }}
          >
            {posts.length} {posts.length === 1 ? 'article' : 'articles'}
          </p>
        </div>

        {posts.length === 0 ? (
          <p
            style={{
              color: MUTED,
              fontSize: 15,
              padding: '48px 0',
              textAlign: 'center',
            }}
          >
            No articles in this category yet.
          </p>
        ) : (
          <div>
            {posts.map((post) => {
              const isOpen = open.has(post.slug);
              return (
                <div className="blog-list-row" key={post.slug}>
                  <button
                    aria-expanded={isOpen}
                    className="blog-list-head"
                    onClick={() => toggle(post.slug)}
                    type="button"
                  >
                    <span className="blog-list-main">
                      <span className="blog-list-meta">
                        {post.featured && (
                          <span className="blog-list-featured">Featured</span>
                        )}
                        <span>{formatDate(post.date)}</span>
                        {post.tags[0] && (
                          <>
                            <span className="blog-list-dot" />
                            <span>
                              {formatCategoryForDisplay(post.tags[0])}
                            </span>
                          </>
                        )}
                        {post.readingTime && (
                          <>
                            <span className="blog-list-dot" />
                            <span>{post.readingTime}</span>
                          </>
                        )}
                      </span>
                      <span className="blog-list-title">{post.title}</span>
                    </span>
                    <ChevronDown
                      aria-hidden
                      className="blog-list-chevron"
                      data-open={isOpen}
                      size={18}
                    />
                  </button>

                  <div className="blog-list-body" data-open={isOpen}>
                    <div className="blog-list-clip">
                      <div className="blog-list-pad">
                        <p className="blog-list-desc">{post.description}</p>
                        <div className="blog-list-footer">
                          <Link
                            className="blog-list-read"
                            href={`/blog/${post.slug}`}
                          >
                            Read full article
                            <ArrowRight size={14} />
                          </Link>
                          {post.author?.name && (
                            <span className="blog-list-author">
                              {post.author.imageUrl && (
                                <span className="blog-list-avatar">
                                  <Image
                                    alt={post.author.name}
                                    className="object-cover"
                                    fill
                                    sizes="24px"
                                    src={post.author.imageUrl}
                                  />
                                </span>
                              )}
                              {post.author.name}
                            </span>
                          )}
                        </div>
                      </div>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>

      <style>{`
        .blog-list-row { border-bottom: 1px solid ${BORDER}; }
        .blog-list-head {
          align-items: center;
          background: transparent;
          border: none;
          cursor: pointer;
          display: grid;
          gap: 20px;
          grid-template-columns: minmax(0, 1fr) auto;
          padding: 22px 8px;
          text-align: left;
          width: 100%;
          transition: background-color 160ms ease;
        }
        .blog-list-head:hover { background: #FBF8F1; }
        .blog-list-head:hover .blog-list-title { color: ${ACCENT}; }
        .blog-list-main {
          display: flex;
          flex-direction: column;
          gap: 8px;
          min-width: 0;
        }
        .blog-list-meta {
          align-items: center;
          color: ${EYEBROW};
          display: flex;
          flex-wrap: wrap;
          font-size: 12px;
          gap: 10px;
          letter-spacing: 0.04em;
        }
        .blog-list-featured {
          color: ${ACCENT};
          font-size: 11px;
          font-weight: 600;
          letter-spacing: 0.12em;
          text-transform: uppercase;
        }
        .blog-list-dot {
          background: ${BORDER};
          border-radius: 50%;
          height: 3px;
          width: 3px;
        }
        .blog-list-title {
          color: ${INK};
          font-size: clamp(1.05rem, 2.2vw, 1.4rem);
          font-weight: 500;
          letter-spacing: -0.01em;
          line-height: 1.25;
          transition: color 160ms ease;
        }
        .blog-list-chevron {
          color: ${EYEBROW};
          flex-shrink: 0;
          transition: transform 260ms ease;
        }
        .blog-list-chevron[data-open='true'] { transform: rotate(180deg); }
        .blog-list-body {
          display: grid;
          grid-template-rows: 0fr;
          transition: grid-template-rows 320ms cubic-bezier(0.22, 0.61, 0.36, 1);
        }
        .blog-list-body[data-open='true'] { grid-template-rows: 1fr; }
        .blog-list-clip { min-height: 0; overflow: hidden; }
        .blog-list-pad {
          max-width: 660px;
          opacity: 0;
          padding: 2px 8px 28px;
          transition: opacity 200ms ease;
        }
        .blog-list-body[data-open='true'] .blog-list-pad {
          opacity: 1;
          transition: opacity 260ms ease 140ms;
        }
        .blog-list-desc {
          color: ${MUTED};
          font-size: 15px;
          line-height: 1.65;
          margin: 0;
        }
        .blog-list-footer {
          align-items: center;
          display: flex;
          flex-wrap: wrap;
          gap: 20px;
          justify-content: space-between;
          margin-top: 18px;
        }
        .blog-list-read {
          align-items: center;
          color: ${INK};
          display: inline-flex;
          font-size: 13px;
          font-weight: 500;
          gap: 6px;
          text-decoration: none;
          transition: color 160ms ease, gap 160ms ease;
        }
        .blog-list-read:hover { color: ${ACCENT}; gap: 9px; }
        .blog-list-author {
          align-items: center;
          color: ${EYEBROW};
          display: inline-flex;
          font-size: 12px;
          gap: 8px;
        }
        .blog-list-avatar {
          border: 1px solid ${BORDER};
          border-radius: 50%;
          height: 24px;
          overflow: hidden;
          position: relative;
          width: 24px;
        }
        @media (max-width: 640px) {
          .blog-list-head { padding: 20px 4px; gap: 12px; }
          .blog-list-pad { padding: 2px 4px 24px; }
        }
      `}</style>
    </section>
  );
}
