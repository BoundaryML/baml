/** biome-ignore-all lint/suspicious/noArrayIndexKey: article blocks and inline runs are static content, never reordered */
import Image from 'next/image';
import { Fragment, type ReactNode } from 'react';
import type { Block } from '@/content/boundary/types';
import CodeBlock from './code-block';

/**
 * Renders the typed block array that content/*.ts exports. There's no markdown parser here on
 * purpose: the blocks are the structure, and the only inline forms are a `code` chip and a
 * [label](href) link, both matched by one regex below.
 */

const INLINE = /(`[^`]+`|\[[^\]\n]+\]\([^)\s]+\))/g;

function inline(text: string): ReactNode {
  const parts = text.split(INLINE).filter(Boolean);
  return parts.map((part, i) => {
    if (part.startsWith('`') && part.endsWith('`')) {
      return (
        <code
          className="font-mono text-[0.9em] px-[5px] py-[2px] rounded-[4px] box-decoration-clone bg-ink/6 text-ink"
          key={i}
        >
          {part.slice(1, -1)}
        </code>
      );
    }
    const link = /^\[([^\]]+)\]\(([^)]+)\)$/.exec(part);
    if (link) {
      const external = /^https?:/.test(link[2]);
      return (
        <a
          className="focus-ring text-ink ulink hover:text-accent"
          href={link[2]}
          key={i}
          {...(external ? { rel: 'noreferrer', target: '_blank' } : {})}
        >
          {link[1]}
        </a>
      );
    }
    return <Fragment key={i}>{part}</Fragment>;
  });
}

/** A heading with the anchor engineers expect: the id is in the data, the # shows on hover or focus. */
function Heading({
  level,
  id,
  text,
}: {
  level: 2 | 3;
  id: string;
  text: string;
}) {
  const Tag = level === 2 ? 'h2' : 'h3';
  const size =
    level === 2
      ? 'text-[22px] md:text-[26px] leading-[1.24] mt-12 mb-3.5'
      : 'text-[18px] md:text-[19px] leading-[1.3] mt-9 mb-2.5';
  return (
    <Tag
      className={`group font-display font-medium tracking-[-0.03em] text-ink first:mt-0 ${size}`}
      id={id}
    >
      {text}
      <a
        aria-label={`Link to this section: ${text}`}
        className="focus-ring ml-2 text-[0.7em] align-middle text-ink-2 opacity-0 transition-opacity group-hover:opacity-100 focus-visible:opacity-100 hover:text-accent"
        href={`#${id}`}
      >
        #
      </a>
    </Tag>
  );
}

export default function Prose({ blocks }: { blocks: Block[] }) {
  return (
    <>
      {blocks.map((b, i) => {
        switch (b.type) {
          case 'heading':
            return <Heading id={b.id} key={i} level={b.level} text={b.text} />;

          case 'paragraph':
            return (
              <p className="my-5 text-[17px] leading-[28px] text-ink-2" key={i}>
                {inline(b.text)}
              </p>
            );

          case 'list': {
            const Tag = b.ordered ? 'ol' : 'ul';
            return (
              <Tag
                className={`my-5 pl-5.5 flex flex-col gap-2 text-[17px] leading-[28px] text-ink-2 marker:text-ink-3 ${
                  b.ordered ? 'list-decimal' : 'list-disc'
                }`}
                key={i}
              >
                {b.items.map((item, j) => (
                  <li key={j}>{inline(item)}</li>
                ))}
              </Tag>
            );
          }

          case 'code':
            return (
              <CodeBlock
                code={b.code}
                filename={b.filename}
                key={i}
                lang={b.lang}
              />
            );

          case 'quote':
            return (
              <blockquote
                className="my-8 pl-5 border-l-2 border-accent font-display text-[19px] md:text-[21px] leading-[1.42] tracking-[-0.02em] text-ink"
                key={i}
              >
                {inline(b.text)}
                {b.cite && (
                  <cite className="block mt-3 font-sans not-italic text-sm leading-5 tracking-normal text-ink-2">
                    {b.cite}
                  </cite>
                )}
              </blockquote>
            );

          case 'image':
            return (
              <figure className="my-8" key={i}>
                <div className="relative aspect-[16/9] rounded-xl overflow-hidden bg-panel">
                  <Image
                    alt={b.alt}
                    className="object-cover"
                    fill
                    sizes="(min-width: 768px) 680px, 100vw"
                    src={b.src}
                  />
                </div>
                {b.caption && (
                  <figcaption className="mt-3 text-sm leading-5 text-ink-2">
                    {b.caption}
                  </figcaption>
                )}
              </figure>
            );
        }
      })}
    </>
  );
}
