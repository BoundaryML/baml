'use client';

import { ChevronDown } from 'lucide-react';
import { type ReactNode, useEffect, useState } from 'react';

export interface TocItem {
  depth?: number;
  href: string;
  label: ReactNode;
}

export function DocsToc({ items }: { items: TocItem[] }) {
  const [activeHref, setActiveHref] = useState(items[0]?.href);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const baseDepth = Math.min(...items.map((item) => item.depth ?? 2));
  const groups: { children: TocItem[]; item: TocItem }[] = [];
  for (const item of items) {
    const parent = groups.at(-1);
    if ((item.depth ?? 2) === baseDepth || !parent) {
      groups.push({ children: [], item });
    } else {
      parent.children.push(item);
    }
  }

  useEffect(() => {
    const headings = items.flatMap((item) => {
      const element = document.getElementById(
        decodeURIComponent(item.href.slice(1)),
      );
      return element ? [{ element, href: item.href }] : [];
    });
    let frame = 0;
    const update = () => {
      frame = 0;
      const headerHeight =
        Number.parseFloat(
          getComputedStyle(document.documentElement).scrollPaddingTop,
        ) || 0;
      const threshold = headerHeight + 40;
      let current = headings[0]?.href;
      for (const heading of headings) {
        if (heading.element.getBoundingClientRect().top > threshold) break;
        current = heading.href;
      }
      setActiveHref(current);
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(update);
    };
    update();
    window.addEventListener('scroll', schedule, { passive: true });
    window.addEventListener('resize', schedule);
    return () => {
      cancelAnimationFrame(frame);
      window.removeEventListener('scroll', schedule);
      window.removeEventListener('resize', schedule);
    };
  }, [items]);

  return (
    <nav aria-label="On this page" className="docs-toc">
      <p className="docs-toc-title">On this page</p>
      {items.length ? (
        <ul>
          {groups.map(({ children, item }) => {
            const containsActive =
              activeHref === item.href ||
              children.some((child) => child.href === activeHref);
            const open = expanded[item.href] ?? containsActive;
            const childrenId = `toc-${item.href.slice(1)}`;
            return (
              <li key={item.href}>
                <div className="docs-toc-row" data-current={containsActive}>
                  <a
                    aria-current={
                      activeHref === item.href ? 'location' : undefined
                    }
                    href={item.href}
                    onClick={() =>
                      setExpanded((previous) => ({
                        ...previous,
                        [item.href]: true,
                      }))
                    }
                  >
                    {item.label}
                  </a>
                  {children.length ? (
                    <button
                      aria-controls={childrenId}
                      aria-expanded={open}
                      className="docs-focus-ring"
                      onClick={() =>
                        setExpanded((previous) => ({
                          ...previous,
                          [item.href]: !open,
                        }))
                      }
                      type="button"
                    >
                      <span className="sr-only">
                        Toggle subsections for {item.label}
                      </span>
                      <ChevronDown aria-hidden="true" />
                    </button>
                  ) : null}
                </div>
                {children.length ? (
                  <ul
                    className="docs-toc-children"
                    hidden={!open}
                    id={childrenId}
                  >
                    {children.map((child) => (
                      <li key={child.href}>
                        <a
                          aria-current={
                            activeHref === child.href ? 'location' : undefined
                          }
                          href={child.href}
                          style={{
                            paddingInlineStart: `${Math.max(0, (child.depth ?? 3) - baseDepth - 1) * 0.6}rem`,
                          }}
                        >
                          {child.label}
                        </a>
                      </li>
                    ))}
                  </ul>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : (
        <span>Overview</span>
      )}
    </nav>
  );
}
