"use client";

// The table of contents of the open page: its top-level sections, and the sub-sections of the one
// being read, which is highlighted, as on BEPs. The page (in its frame) reports which heading is in
// view; the frame passes that on as a "bcs-active-heading" window event.
import { useEffect, useMemo, useState } from "react";

import { type Heading, hasContents, sectionOf, sections } from "@/lib/bcs-outline";

export function PageContents({ headings }: { headings: Heading[] }) {
  const list = useMemo(() => sections(headings), [headings]);
  const [active, setActive] = useState<string | null>(null);

  useEffect(() => {
    const onActive = (event: Event) => setActive((event as CustomEvent<string | null>).detail);
    window.addEventListener("bcs-active-heading", onActive);
    return () => window.removeEventListener("bcs-active-heading", onActive);
  }, []);

  // The section being read: the one containing the heading in view, however deep that heading is.
  const openId = sectionOf(headings, active);
  const open = list.find((section) => section.id === openId);
  if (!hasContents(headings)) return null;

  const link = (heading: Heading, nested: boolean) => (
    // A plain #link: the frame scrolls to it when clicked (frame.tsx), even if already in the address.
    <a
      href={`#${heading.id}`}
      onClick={() => setActive(heading.id)}
      aria-current={active === heading.id ? "location" : undefined}
      title={heading.text}
      className={`block truncate rounded py-0.5 pr-2 hover:bg-muted ${nested ? "pl-5 text-xs" : "pl-2"} ${
        active === heading.id ? "font-medium text-foreground" : "text-muted-foreground hover:text-foreground"
      }`}
    >
      {heading.text}
    </a>
  );

  return (
    <nav aria-label="On this page">
      <h2 className="px-2 pb-1 text-xs font-medium uppercase tracking-wide text-muted-foreground">
        On this page
      </h2>
      <ul className="space-y-0.5">
        {list.map((section) => (
          <li key={section.id}>
            {link(section, false)}
            {section === open && section.children.length > 0 && (
              <ul className="mt-0.5 space-y-0.5 border-l ml-2">
                {section.children.map((child) => (
                  <li key={child.id}>{link(child, true)}</li>
                ))}
              </ul>
            )}
          </li>
        ))}
      </ul>
    </nav>
  );
}
