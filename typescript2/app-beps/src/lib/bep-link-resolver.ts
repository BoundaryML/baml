import { buildBepPath } from "@/lib/bep-routes";

export interface BepLinkContext {
  bepNumber: number;
  isHistorical: boolean;
  versionNumber: number | null;
}

export interface ResolvedBepLink {
  href: string;
  isExternal: boolean;
  isInternalBepLink: boolean;
}

const EXTERNAL_PROTOCOL_PATTERN =
  /^(https?:\/\/|mailto:|tel:|sms:|data:|ftp:\/\/|\/\/)/i;
const UNSAFE_PROTOCOL_PATTERN = /^(javascript:|vbscript:)/i;

function isExternalHref(href: string): boolean {
  return EXTERNAL_PROTOCOL_PATTERN.test(href);
}

function splitHref(href: string): { path: string; hash: string } {
  const [path, hash] = href.split("#", 2);
  return { path, hash: hash ? `#${hash}` : "" };
}

function normalizePath(path: string): string {
  return path
    .trim()
    .replace(/^\.\/+/, "")
    .replace(/^\/+/, "/")
    .replace(/\/{2,}/g, "/");
}

function extractPageSlug(path: string): string | null {
  const safeDecode = (value: string): string | null => {
    try {
      return decodeURIComponent(value);
    } catch {
      return null;
    }
  };

  const cleaned = path.trim().replace(/^\.\/+/, "").replace(/^\/+/, "");
  const lower = cleaned.toLowerCase();

  const pagesPath = lower.match(/^pages\/([^/]+)\.md$/i);
  if (pagesPath?.[1]) {
    const decoded = safeDecode(pagesPath[1]);
    return decoded ? decoded.toLowerCase() : null;
  }

  const parentPagesPath = lower.match(/^\.\.\/pages\/([^/]+)\.md$/i);
  if (parentPagesPath?.[1]) {
    const decoded = safeDecode(parentPagesPath[1]);
    return decoded ? decoded.toLowerCase() : null;
  }

  const directFile = lower.match(/^([^/]+)\.md$/i);
  if (directFile?.[1] && directFile[1] !== "readme") {
    const decoded = safeDecode(directFile[1]);
    return decoded ? decoded.toLowerCase() : null;
  }

  return null;
}

function isReadmePath(path: string): boolean {
  const cleaned = path.trim().replace(/^\.\/+/, "").replace(/^\/+/, "");
  return /^(\.\.\/)?readme\.md$/i.test(cleaned);
}

export function resolveBepLink(
  href: string | undefined,
  context?: BepLinkContext
): ResolvedBepLink {
  if (!href) {
    return { href: "", isExternal: false, isInternalBepLink: false };
  }

  if (UNSAFE_PROTOCOL_PATTERN.test(href.trim())) {
    return { href: "#", isExternal: false, isInternalBepLink: false };
  }

  if (isExternalHref(href)) {
    return { href, isExternal: true, isInternalBepLink: false };
  }

  if (!context) {
    return { href, isExternal: false, isInternalBepLink: false };
  }

  if (href.startsWith("#")) {
    return { href, isExternal: false, isInternalBepLink: false };
  }

  const { path, hash } = splitHref(href);
  const normalizedPath = normalizePath(path);
  const versionNumber =
    context.isHistorical && context.versionNumber !== null
      ? context.versionNumber
      : null;

  if (!normalizedPath || isReadmePath(normalizedPath)) {
    return {
      href: `${buildBepPath({
        bepNumber: context.bepNumber,
        section: "readme",
        versionNumber,
      })}${hash}`,
      isExternal: false,
      isInternalBepLink: true,
    };
  }

  const pageSlug = extractPageSlug(normalizedPath);
  if (pageSlug) {
    return {
      href: `${buildBepPath({
        bepNumber: context.bepNumber,
        section: "page",
        pageSlug,
        versionNumber,
      })}${hash}`,
      isExternal: false,
      isInternalBepLink: true,
    };
  }

  return { href, isExternal: false, isInternalBepLink: false };
}
