export const MAIN_CONTENT_ID = "_main";

export const RESERVED_PAGE_SLUGS = new Set([
  "issues",
  "decisions",
  "ai",
  "pages",
  "v",
  "readme",
]);

export type BepSectionType = "readme" | "page" | "issues" | "decisions" | "ai";

export interface ParsedBepRoute {
  section: BepSectionType;
  pageSlug?: string;
  versionNumber: number | null;
  isHistorical: boolean;
  isValid: boolean;
}

function parseVersionSegment(raw: string): number | null {
  if (!/^\d+$/.test(raw)) {
    return null;
  }
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : null;
}

export function parseBepSegments(segments: string[]): ParsedBepRoute {
  if (segments.length === 0) {
    return {
      section: "readme",
      versionNumber: null,
      isHistorical: false,
      isValid: true,
    };
  }

  if (segments[0] === "v") {
    if (segments.length < 2) {
      return {
        section: "readme",
        versionNumber: null,
        isHistorical: false,
        isValid: false,
      };
    }

    const versionNumber = parseVersionSegment(segments[1]);
    if (!versionNumber) {
      return {
        section: "readme",
        versionNumber: null,
        isHistorical: false,
        isValid: false,
      };
    }

    if (segments.length === 2) {
      return {
        section: "readme",
        versionNumber,
        isHistorical: true,
        isValid: true,
      };
    }

    if (segments.length === 4 && segments[2] === "pages" && segments[3]) {
      return {
        section: "page",
        pageSlug: segments[3],
        versionNumber,
        isHistorical: true,
        isValid: true,
      };
    }

    return {
      section: "readme",
      versionNumber,
      isHistorical: true,
      isValid: false,
    };
  }

  if (segments[0] === "pages" && segments.length === 2 && segments[1]) {
    return {
      section: "page",
      pageSlug: segments[1],
      versionNumber: null,
      isHistorical: false,
      isValid: true,
    };
  }

  if (segments.length === 1 && segments[0] === "issues") {
    return {
      section: "issues",
      versionNumber: null,
      isHistorical: false,
      isValid: true,
    };
  }

  if (segments.length === 1 && segments[0] === "decisions") {
    return {
      section: "decisions",
      versionNumber: null,
      isHistorical: false,
      isValid: true,
    };
  }

  if (segments.length === 1 && segments[0] === "ai") {
    return {
      section: "ai",
      versionNumber: null,
      isHistorical: false,
      isValid: true,
    };
  }

  return {
    section: "readme",
    versionNumber: null,
    isHistorical: false,
    isValid: false,
  };
}

export function buildBepPath({
  bepNumber,
  section,
  pageSlug,
  versionNumber,
}: {
  bepNumber: number;
  section: BepSectionType;
  pageSlug?: string;
  versionNumber?: number | null;
}): string {
  const base = `/beps/${bepNumber}`;
  const isHistorical = versionNumber !== undefined && versionNumber !== null;

  if (isHistorical) {
    const historicalPrefix = `${base}/v/${versionNumber}`;
    if (section === "page" && pageSlug) {
      return `${historicalPrefix}/pages/${pageSlug}`;
    }
    return historicalPrefix;
  }

  if (section === "page" && pageSlug) {
    return `${base}/pages/${pageSlug}`;
  }
  if (section === "issues") {
    return `${base}/issues`;
  }
  if (section === "decisions") {
    return `${base}/decisions`;
  }
  if (section === "ai") {
    return `${base}/ai`;
  }
  return base;
}

export function toNavSectionId(
  section: BepSectionType,
  pageSlug?: string
): string {
  if (section === "readme") return MAIN_CONTENT_ID;
  if (section === "page" && pageSlug) return pageSlug;
  if (section === "ai") return "ai";
  return section;
}

export function isReservedPageSlug(slug: string): boolean {
  return RESERVED_PAGE_SLUGS.has(slug);
}
