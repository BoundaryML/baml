// Turns a BCS page into the HTML document the sandboxed frame shows. Server only.
//
// An HTML page is shown as written. A Markdown page goes through the same plugins as BEPs (GitHub
// tables and lists, raw HTML kept), so a proposal can paste its illustrations (inline SVG, a
// <style> block) between paragraphs and they look exactly as designed: the frame isolates them
// from the site's styles, and the site from theirs.
import { unified } from "unified";
import remarkParse from "remark-parse";
import remarkGfm from "remark-gfm";
import remarkRehype from "remark-rehype";
import rehypeRaw from "rehype-raw";
import rehypeStringify from "rehype-stringify";
import type { Element, ElementContent, Root, RootContent } from "hast";

export type BcsPageFormat = "markdown" | "html";

/** Mermaid, loaded inside the frame only when a page has a ```mermaid block; diagrams sit on a light card in both themes. */
const MERMAID_SCRIPT = "https://cdn.jsdelivr.net/npm/mermaid@11.16.0/dist/mermaid.min.js";

type RenderOptions = {
  format: BcsPageFormat;
  content: string;
  title: string;
  /** Where a link to another page of the proposal goes, by its slug ("" for the main page). */
  pageHref: (slug: string) => string;
};

/** The document for a page, ready for the frame's srcdoc or the raw route. */
export async function renderBcsPage(options: RenderOptions): Promise<string> {
  if (options.format === "html") {
    // A page written without a doctype (an artifact, say) was designed in standards mode; on its
    // own a browser would render it in quirks mode, with different table and line spacing.
    return /^\s*(<!--[\s\S]*?-->\s*)*<!doctype/i.test(options.content)
      ? options.content
      : `<!doctype html>\n${options.content}`;
  }
  const state = { mermaid: false };
  // A page that brings its own stylesheet is laid out by it alone: the defaults would fight it.
  const ownStyles = /<style[\s>]/i.test(options.content);
  const body = String(
    await unified()
      .use(remarkParse)
      .use(remarkGfm)
      .use(remarkRehype, { allowDangerousHtml: true })
      .use(rehypeRaw)
      .use(() => (tree: Root) => adjust(tree, options.pageHref, state))
      .use(rehypeStringify)
      .process(options.content)
  );
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(options.title)}</title>
<base target="_blank">
${ownStyles ? "" : `<style>${DEFAULT_CSS}</style>`}
</head>
<body>
<article${ownStyles ? "" : ' class="bcs-page"'}>
${body}
</article>
<script>${BRIDGE_SCRIPT}</script>
${
  state.mermaid
    ? `<script src="${MERMAID_SCRIPT}"></script>
<script>mermaid.initialize({ startOnLoad: true, securityLevel: "strict", theme: "default" });</script>`
    : ""
}
</body>
</html>`;
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/**
 * A link to another page of the proposal, as the `bep` folder lays them out: `pages/<slug>.md`
 * (or `.html`, possibly nested), `./<slug>.md`, and `index.md` or `README.md` for the main page.
 * Returns the slug and any #fragment, or null for any other link.
 */
export function pageLink(href: string): { slug: string; hash: string } | null {
  const match = href.match(/^(?:\.\/|\.\.\/)*(?:pages\/)?(?:[a-z0-9-]+\/)*([A-Za-z0-9-]+)\.(?:md|html?)(#.*)?$/);
  if (!match) return null;
  const name = match[1];
  const slug = /^(index|readme)$/i.test(name) ? "" : name.toLowerCase();
  return { slug, hash: match[2] ?? "" };
}

function text(node: ElementContent | RootContent): string {
  if (node.type === "text") return node.value;
  if ("children" in node) return (node.children as ElementContent[]).map(text).join("");
  return "";
}

function headingId(value: string): string {
  return value
    .toLowerCase()
    .replace(/[^a-z0-9\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

/**
 * Marks links between pages for the site to open, keeps #fragment links in the frame, gives
 * headings ids for those links, and turns ```mermaid blocks into diagrams.
 */
function adjust(tree: Root, pageHref: (slug: string) => string, state: { mermaid: boolean }) {
  const used = new Map<string, number>();
  const walk = (node: Root | Element) => {
    node.children.forEach((child, index) => {
      if (child.type !== "element") return;
      if (child.tagName === "a" && typeof child.properties.href === "string") {
        const href = child.properties.href;
        const link = pageLink(href);
        if (href.startsWith("#")) {
          child.properties.target = "_self";
        } else if (link) {
          // The site opens it (see BRIDGE_SCRIPT); a modifier-click opens it in a new tab.
          child.properties.href = pageHref(link.slug) + link.hash;
          child.properties.dataBcsPage = "";
        }
      }
      if (/^h[1-6]$/.test(child.tagName) && child.properties.id === undefined) {
        const base = headingId(text(child)) || "section";
        const seen = used.get(base) ?? 0;
        used.set(base, seen + 1);
        child.properties.id = seen === 0 ? base : `${base}-${seen}`;
      }
      const code = child.tagName === "pre" ? child.children.find((each) => each.type === "element") : undefined;
      if (
        code?.type === "element" &&
        code.tagName === "code" &&
        Array.isArray(code.properties.className) &&
        code.properties.className.includes("language-mermaid")
      ) {
        state.mermaid = true;
        const diagram: Element = {
          type: "element",
          tagName: "pre",
          properties: { className: ["mermaid"] },
          children: [{ type: "text", value: text(code) }],
        };
        node.children[index] = diagram;
        return;
      }
      walk(child);
    });
  };
  walk(tree);
}

/**
 * Runs in the frame, beside the page's own scripts. The frame cannot navigate the site, so a click
 * on a link to another page of the proposal asks the site to open it (the site checks it is one of
 * this proposal's pages), and the site asks the frame to scroll to a #fragment once it loads.
 * Shown on its own (the raw route), the page has no site around it and links work as usual.
 */
const BRIDGE_SCRIPT = `
(function () {
  if (window.parent === window) return;
  document.addEventListener("click", function (event) {
    var link = event.target && event.target.closest ? event.target.closest("a[data-bcs-page]") : null;
    if (!link || event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    window.parent.postMessage({ type: "bcs-open-page", href: link.getAttribute("href") }, "*");
  });
  window.addEventListener("message", function (event) {
    if (event.source !== window.parent || !event.data || event.data.type !== "bcs-scroll") return;
    var target = document.getElementById(String(event.data.id));
    if (target) target.scrollIntoView();
  });
})();
`;

/** How a Markdown page looks when it brings no <style> of its own: close to a BEP. */
const DEFAULT_CSS = `
:root { color-scheme: light dark; --fg: #1f2328; --muted: #59636e; --border: #d1d9e0; --bg: #ffffff; --code: #f6f8fa; --link: #0969da; }
@media (prefers-color-scheme: dark) {
  :root { --fg: #e6edf3; --muted: #9198a1; --border: #3d444d; --bg: #0d1117; --code: #151b23; --link: #4493f8; }
}
* { box-sizing: border-box; }
html { background: var(--bg); }
body { margin: 0; color: var(--fg); background: var(--bg); font: 16px/1.65 ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif; }
.bcs-page { max-width: 880px; margin: 0 auto; padding: 32px 24px 96px; }
h1, h2, h3, h4 { line-height: 1.25; margin: 1.6em 0 0.6em; scroll-margin-top: 16px; }
h1 { font-size: 2em; margin-top: 0; }
h2 { font-size: 1.5em; padding-bottom: 0.3em; border-bottom: 1px solid var(--border); }
h3 { font-size: 1.25em; }
p, ul, ol, blockquote, table, pre, figure { margin: 0 0 1em; }
ul, ol { padding-left: 1.6em; }
li + li { margin-top: 0.25em; }
a { color: var(--link); }
blockquote { margin-left: 0; padding: 0 1em; color: var(--muted); border-left: 4px solid var(--border); }
code { font: 0.875em/1.5 ui-monospace, SFMono-Regular, Menlo, monospace; background: var(--code); padding: 0.15em 0.35em; border-radius: 6px; }
pre { background: var(--code); padding: 14px 16px; border-radius: 8px; overflow-x: auto; }
pre code { background: none; padding: 0; }
pre.mermaid { background: #ffffff; color: #1f2328; text-align: center; border: 1px solid var(--border); }
table { border-collapse: collapse; display: block; overflow-x: auto; }
th, td { border: 1px solid var(--border); padding: 6px 12px; text-align: left; vertical-align: top; }
th { background: var(--code); }
img, svg, video { max-width: 100%; height: auto; }
hr { border: 0; border-top: 1px solid var(--border); margin: 2em 0; }
`;
