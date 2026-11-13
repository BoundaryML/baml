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

export type RenderedPage = {
  /** The document for the frame's srcdoc or the raw route. */
  document: string;
  /** The ids of the blocks comments can be attached to, in reading order: none for HTML pages. */
  blocks: string[];
};

/** Renders a page for the sandboxed frame. */
export async function renderBcsPage(options: RenderOptions): Promise<RenderedPage> {
  if (options.format === "html") {
    // A page written without a doctype (an artifact, say) was designed in standards mode; on its
    // own a browser would render it in quirks mode, with different table and line spacing.
    const page = /^\s*(<!--[\s\S]*?-->\s*)*<!doctype/i.test(options.content)
      ? options.content
      : `<!doctype html>\n${options.content}`;
    // Keep its #links inside the frame (see FRAGMENT_SCRIPT). Appended rather than placed before
    // </body>, which could be text inside the page's own script; browsers run it in the body.
    return { document: `${page}\n<script>${FRAGMENT_SCRIPT}</script>`, blocks: [] };
  }
  const blocks: string[] = [];
  // A page that brings its own stylesheet is laid out by it alone: the defaults would fight it.
  const state = { mermaid: false, ownStyles: false };
  const body = String(
    await unified()
      .use(remarkParse)
      .use(remarkGfm)
      .use(remarkRehype, { allowDangerousHtml: true })
      .use(rehypeRaw)
      .use(() => (tree: Root) => {
        adjust(tree, options.pageHref, state);
        blocks.push(...markBlocks(tree));
      })
      .use(rehypeStringify)
      .process(options.content)
  );
  const document = `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>${escapeHtml(options.title)}</title>
<base target="_blank">
${state.ownStyles ? "" : `<style>${DEFAULT_CSS}</style>`}
</head>
<body>
<article${state.ownStyles ? "" : ' class="bcs-page"'}>
${body}
</article>
<script>${FRAGMENT_SCRIPT}${BRIDGE_SCRIPT}</script>
${
  state.mermaid
    ? `<script src="${MERMAID_SCRIPT}"></script>
<script>mermaid.initialize({ startOnLoad: true, securityLevel: "strict", theme: "default" });</script>`
    : ""
}
</body>
</html>`;
  return { document, blocks };
}

function escapeHtml(text: string): string {
  return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/**
 * A link to another page of the proposal, as the `bep` folder lays them out: `pages/<slug>.md`
 * (or `.markdown` or `.html`, possibly nested), `./<slug>.md`, and `index.md` or `README.md` for the main page.
 * Returns the slug and any #fragment, or null for any other link.
 */
export function pageLink(href: string): { slug: string; hash: string } | null {
  const match = href.match(/^(?:\.\/|\.\.\/)*(?:pages\/)?(?:[a-z0-9-]+\/)*([A-Za-z0-9-]+)\.(?:md|markdown|html?)(#.*)?$/);
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
 * Marks links between pages for the site to open, gives headings ids for #fragment links, notes a
 * page's own stylesheet, and turns ```mermaid blocks into diagrams.
 */
function adjust(
  tree: Root,
  pageHref: (slug: string) => string,
  state: { mermaid: boolean; ownStyles: boolean }
) {
  const used = new Map<string, number>();
  const walk = (node: Root | Element) => {
    node.children.forEach((child, index) => {
      if (child.type !== "element") return;
      // A real stylesheet, not one shown in a code example (that is text inside <code>).
      if (child.tagName === "style") state.ownStyles = true;
      if (child.tagName === "a" && typeof child.properties.href === "string") {
        const href = child.properties.href;
        const link = pageLink(href);
        if (link) {
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

/** Elements a comment can be attached to; one inside another belongs to the outer one. */
const BLOCK_TAGS = new Set([
  "p", "h1", "h2", "h3", "h4", "h5", "h6", "li", "pre", "blockquote", "table", "figure", "svg",
  "img", "dl", "details",
]);

/** A short, stable hash (FNV-1a, base 36). */
function hash(value: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < value.length; i++) {
    h ^= value.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(36);
}

function withoutPositions(node: unknown): unknown {
  if (Array.isArray(node)) return node.map(withoutPositions);
  if (node && typeof node === "object") {
    return Object.fromEntries(
      Object.entries(node).filter(([key]) => key !== "position").map(([key, value]) => [key, withoutPositions(value)])
    );
  }
  return node;
}

/**
 * Gives each commentable block a `data-bcs-block` id from its tag and text, so a comment finds its
 * block again in later versions as long as the block's text is unchanged. Returns the ids.
 */
function markBlocks(tree: Root): string[] {
  const ids: string[] = [];
  const seen = new Map<string, number>();
  const walk = (node: Root | Element) => {
    for (const child of node.children) {
      if (child.type !== "element" || child.tagName === "style" || child.tagName === "script") continue;
      if (BLOCK_TAGS.has(child.tagName)) {
        const words = text(child).replace(/\s+/g, " ").trim().slice(0, 400);
        const key = `${child.tagName}:${words || JSON.stringify(withoutPositions(child)).slice(0, 2000)}`;
        const base = `${child.tagName}-${hash(key)}`;
        const count = seen.get(base) ?? 0;
        seen.set(base, count + 1);
        const id = count === 0 ? base : `${base}-${count + 1}`;
        child.properties.dataBcsBlock = id;
        ids.push(id);
        continue;
      }
      walk(child);
    }
  };
  walk(tree);
  return ids;
}

/**
 * Runs in the frame, on every page, HTML ones included. A srcdoc document resolves links against
 * the site's address, so `href="#intro"` would load the site in the frame instead of scrolling:
 * this scrolls to the element instead, and scrolls to the #fragment the site asks for once the
 * page loads. Shown on its own (the raw route), the page has a real address and needs none.
 */
const FRAGMENT_SCRIPT = `
(function () {
  if (window.parent === window) return;
  function scrollTo(id) {
    var target = id ? document.getElementById(id) || document.getElementsByName(id)[0] : null;
    if (target) target.scrollIntoView();
    else if (!id) window.scrollTo(0, 0);
  }
  document.addEventListener("click", function (event) {
    var link = event.target && event.target.closest ? event.target.closest("a[href^='#']") : null;
    if (!link || event.defaultPrevented || event.button !== 0) return;
    event.preventDefault();
    scrollTo(decodeURIComponent(link.getAttribute("href").slice(1)));
  });
  window.addEventListener("message", function (event) {
    if (event.source !== window.parent || !event.data || event.data.type !== "bcs-scroll") return;
    scrollTo(String(event.data.id));
  });
})();
`;

/**
 * Runs in the frame, beside the page's own scripts, and is all the frame and the site say to each
 * other. The frame cannot navigate the site, so a click on a link to another of the proposal's
 * pages asks the site to open it (the site checks it is one of this proposal's pages). Hovering a
 * block offers a Comment button, which asks the site to start a thread on it; the site sends back
 * how many open threads each block has, shown as badges, and asks the frame to scroll to a block
 * (FRAGMENT_SCRIPT handles #fragments). Shown on its own (the raw route), the page has no site
 * around it and does none of this. Its own styles cannot restyle these controls: they reset every
 * property inline.
 */
const BRIDGE_SCRIPT = `
(function () {
  if (window.parent === window) return;
  function send(data) { window.parent.postMessage(data, "*"); }

  document.addEventListener("click", function (event) {
    var link = event.target && event.target.closest ? event.target.closest("a[data-bcs-page]") : null;
    if (!link || event.defaultPrevented || event.button !== 0) return;
    if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    send({ type: "bcs-open-page", href: link.getAttribute("href") });
  });

  var BASE = "all:initial;position:absolute;box-sizing:border-box;display:none;cursor:pointer;" +
    "font:600 12px/1 system-ui,-apple-system,Segoe UI,sans-serif;border-radius:999px;padding:5px 9px;" +
    "box-shadow:0 1px 3px rgba(0,0,0,0.25);white-space:nowrap;";
  var layer = document.createElement("div");
  layer.setAttribute("data-bcs-ui", "");
  layer.style.cssText = "all:initial;position:absolute;top:0;left:0;width:0;height:0;z-index:2147483647;";
  document.body.appendChild(layer);
  function control(label, colors) {
    var el = document.createElement("button");
    el.type = "button";
    el.textContent = label;
    el.style.cssText = BASE + colors;
    layer.appendChild(el);
    return el;
  }
  function blockOf(node) {
    return node && node.closest ? node.closest("[data-bcs-block]") : null;
  }
  function find(id) {
    var all = document.querySelectorAll("[data-bcs-block]");
    for (var i = 0; i < all.length; i++) if (all[i].getAttribute("data-bcs-block") === id) return all[i];
    return null;
  }
  function place(el, block, shift) {
    var r = block.getBoundingClientRect();
    var x = Math.min(r.right + 8 + shift, document.documentElement.clientWidth - 110);
    el.style.left = Math.max(4, x + window.scrollX) + "px";
    el.style.top = (r.top + window.scrollY) + "px";
    el.style.display = "block";
  }

  var counts = {};
  var badges = [];
  function drawBadges() {
    badges.forEach(function (badge) { badge.remove(); });
    badges = [];
    Object.keys(counts).forEach(function (id) {
      var block = find(id);
      if (!block || !counts[id]) return;
      var badge = control("\u{1F4AC} " + counts[id], "background:#6a45bf;color:#ffffff;border:1px solid #6a45bf;");
      badge.title = counts[id] + " open comment" + (counts[id] === 1 ? "" : "s");
      badge.addEventListener("click", function () { send({ type: "bcs-show-thread", block: id }); });
      place(badge, block, 0);
      badges.push(badge);
    });
  }

  var add = control("+ Comment", "background:#ffffff;color:#5b3aa8;border:1px solid #cfc4ef;");
  var current = null;
  var hideTimer = null;
  document.addEventListener("mouseover", function (event) {
    if (event.target && event.target.closest && event.target.closest("[data-bcs-ui]")) {
      clearTimeout(hideTimer);
      return;
    }
    var block = blockOf(event.target);
    clearTimeout(hideTimer);
    if (!block) {
      hideTimer = setTimeout(function () { add.style.display = "none"; }, 600);
      return;
    }
    current = block;
    place(add, block, counts[block.getAttribute("data-bcs-block")] ? 58 : 0);
  });
  add.addEventListener("click", function () {
    if (!current) return;
    var words = (current.innerText || current.textContent || "").replace(/\\s+/g, " ").trim();
    send({ type: "bcs-comment", block: current.getAttribute("data-bcs-block"), text: words.slice(0, 280) });
  });

  function flash(block) {
    var outline = block.style.outline;
    var offset = block.style.outlineOffset;
    block.style.outline = "2px solid #7c3aed";
    block.style.outlineOffset = "4px";
    setTimeout(function () { block.style.outline = outline; block.style.outlineOffset = offset; }, 1800);
  }

  window.addEventListener("message", function (event) {
    if (event.source !== window.parent || !event.data) return;
    var data = event.data;
    if (data.type === "bcs-marks" && data.counts && typeof data.counts === "object") {
      counts = data.counts;
      drawBadges();
    } else if (data.type === "bcs-focus") {
      var block = find(String(data.block));
      if (block) {
        block.scrollIntoView({ block: "center" });
        flash(block);
      }
    }
  });
  window.addEventListener("resize", drawBadges);
  window.addEventListener("load", drawBadges);
  if (window.ResizeObserver) new ResizeObserver(drawBadges).observe(document.body);
  send({ type: "bcs-ready" });
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
