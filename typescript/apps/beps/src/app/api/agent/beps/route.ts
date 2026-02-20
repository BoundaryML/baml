import { NextRequest, NextResponse } from "next/server";
import { ConvexHttpClient } from "convex/browser";
import type { FunctionReturnType } from "convex/server";
import type { Id } from "../../../../../convex/_generated/dataModel";
import { api } from "../../../../../convex/_generated/api";
import {
  type ExportData,
  type ExportFile,
  generateAllExportFiles,
} from "@/lib/export-utils";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

type ListBepResult = {
  _id: string;
  number: number;
  title: string;
  status: string;
  updatedAt: number;
};

type MatchResult = {
  bep: ListBepResult;
  score: number;
};

const TRUE_VALUES = new Set(["1", "true", "yes", "y", "on"]);
const BEP_LIST_LIMIT = 500;
const BEP_LIST_PROBE_LIMIT = BEP_LIST_LIMIT + 1;

type RawBepListItem = FunctionReturnType<typeof api.beps.list>[number];

function isTruthy(value: string | null): boolean {
  if (!value) return false;
  return TRUE_VALUES.has(value.toLowerCase());
}

function normalize(value: string): string {
  return value.toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}

function formatBepId(number: number): string {
  return `BEP-${String(number).padStart(3, "0")}`;
}

function levenshteinDistance(a: string, b: string): number {
  if (a === b) return 0;
  if (a.length === 0) return b.length;
  if (b.length === 0) return a.length;

  const prev = new Array<number>(b.length + 1);
  const curr = new Array<number>(b.length + 1);

  for (let j = 0; j <= b.length; j += 1) {
    prev[j] = j;
  }

  for (let i = 1; i <= a.length; i += 1) {
    curr[0] = i;
    for (let j = 1; j <= b.length; j += 1) {
      const cost = a[i - 1] === b[j - 1] ? 0 : 1;
      curr[j] = Math.min(
        curr[j - 1] + 1,
        prev[j] + 1,
        prev[j - 1] + cost
      );
    }
    for (let j = 0; j <= b.length; j += 1) {
      prev[j] = curr[j];
    }
  }

  return prev[b.length];
}

function tokenOverlapScore(query: string, candidate: string): number {
  const queryTokens = new Set(query.split(" ").filter(Boolean));
  if (queryTokens.size === 0) return 0;

  const candidateTokens = new Set(candidate.split(" ").filter(Boolean));
  let overlap = 0;
  for (const token of queryTokens) {
    if (candidateTokens.has(token)) overlap += 1;
  }

  return overlap / queryTokens.size;
}

function scoreBepMatch(query: string, bep: ListBepResult): number {
  const normalizedQuery = normalize(query);
  if (!normalizedQuery) return 0;

  const id = normalize(formatBepId(bep.number));
  const title = normalize(bep.title);
  const full = `${id} ${title}`.trim();

  let score = 0;

  if (normalizedQuery === id) score += 1.2;
  if (normalizedQuery === title) score += 1.1;
  if (normalizedQuery === full) score += 1.3;

  if (id.includes(normalizedQuery)) score += 0.85;
  if (title.includes(normalizedQuery)) score += 0.75;
  if (full.includes(normalizedQuery)) score += 0.65;

  score += tokenOverlapScore(normalizedQuery, full) * 0.7;

  const distance = levenshteinDistance(normalizedQuery, title);
  const maxLen = Math.max(normalizedQuery.length, title.length, 1);
  const similarity = 1 - distance / maxLen;
  score += Math.max(0, similarity) * 0.3;

  return score;
}

function findBestMatch(query: string, beps: ListBepResult[]): {
  bestMatch: MatchResult | null;
  topMatches: MatchResult[];
} {
  const normalizedQuery = normalize(query);

  // Match "bep 5" / "BEP-005" anywhere, or a bare integer when that is the entire query.
  const byNumber = normalizedQuery.match(
    /(?:^|\s)bep\s*0*(\d+)(?:\s|$)|^0*(\d+)$/
  );
  const numberCandidate = byNumber?.[1] ?? byNumber?.[2];
  if (numberCandidate) {
    const parsedNumber = Number.parseInt(numberCandidate, 10);
    if (!Number.isNaN(parsedNumber)) {
      const exact = beps.find((bep) => bep.number === parsedNumber);
      if (exact) {
        return {
          bestMatch: { bep: exact, score: 99 },
          topMatches: [{ bep: exact, score: 99 }],
        };
      }
    }
  }

  const scored = beps
    .map((bep) => ({ bep, score: scoreBepMatch(query, bep) }))
    .sort((a, b) => b.score - a.score);

  return {
    bestMatch: scored.length > 0 ? scored[0] : null,
    topMatches: scored.slice(0, 5),
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

function isExportData(value: unknown): value is ExportData {
  if (!isRecord(value)) return false;

  return (
    isRecord(value.bep) &&
    Array.isArray(value.pages) &&
    Array.isArray(value.comments) &&
    Array.isArray(value.decisions) &&
    Array.isArray(value.issues) &&
    Array.isArray(value.versions) &&
    Array.isArray(value.summaries) &&
    typeof value.currentVersion === "number" &&
    typeof value.exportedAt === "number"
  );
}

// Callers should pass only markdown files.
function flattenMarkdownForAgents(files: ExportFile[]): string {
  return files
    .map(
      (file) =>
        `<!-- FILE: ${file.path} -->\n${file.content.trimEnd()}`
    )
    .join("\n\n");
}

function jsonResponse(body: unknown, status = 200): NextResponse {
  return NextResponse.json(body, {
    status,
    headers: {
      ...CORS_HEADERS,
      "Cache-Control": "no-store",
    },
  });
}

export async function OPTIONS(): Promise<Response> {
  return new Response(null, {
    status: 204,
    headers: CORS_HEADERS,
  });
}

export async function GET(request: NextRequest): Promise<Response> {
  const convexUrl = process.env.NEXT_PUBLIC_CONVEX_URL;
  if (!convexUrl) {
    return jsonResponse(
      { error: "Missing NEXT_PUBLIC_CONVEX_URL environment variable." },
      500
    );
  }

  const convex = new ConvexHttpClient(convexUrl);
  const searchParams = request.nextUrl.searchParams;
  const rawQuery =
    searchParams.get("name") ??
    searchParams.get("query") ??
    searchParams.get("q");
  const query = rawQuery?.trim() ?? "";
  const normalizedQuery = normalize(query);
  const omitOtherVersions = isTruthy(searchParams.get("omitOtherVersions"));
  const format = (searchParams.get("format") ?? "json").toLowerCase();

  let beps: ListBepResult[];
  try {
    const bepsRaw = await convex.query(api.beps.list, {
      limit: BEP_LIST_PROBE_LIMIT,
    });
    if (bepsRaw.length > BEP_LIST_LIMIT) {
      return jsonResponse(
        {
          error: `BEP list exceeds the supported search size (${BEP_LIST_LIMIT}).`,
          detail: "Add pagination support before returning more BEPs.",
        },
        503
      );
    }

    beps = bepsRaw.map(
      (bep: RawBepListItem) => ({
        _id: String(bep._id),
        number: bep.number,
        title: bep.title,
        status: bep.status,
        updatedAt: bep.updatedAt,
      })
    );
  } catch (err) {
    return jsonResponse(
      {
        error: "Failed to fetch BEP list.",
        detail: err instanceof Error ? err.message : String(err),
      },
      502
    );
  }

  if (!query) {
    const sorted = [...beps].sort((a, b) => a.number - b.number);

    return jsonResponse({
      mode: "list",
      total: sorted.length,
      beps: sorted.map((bep) => ({
        id: formatBepId(bep.number),
        number: bep.number,
        title: bep.title,
        status: bep.status,
        updatedAt: new Date(bep.updatedAt).toISOString(),
      })),
      usage: {
        list: "/api/agent/beps",
        fetch: "/api/agent/beps?name=<bep-name-or-id>",
        omitOtherVersions:
          "/api/agent/beps?name=<bep-name-or-id>&omitOtherVersions=true",
      },
    });
  }

  const { bestMatch, topMatches } = findBestMatch(query, beps);
  const minimumScore = 0.6;
  if (!bestMatch || bestMatch.score < minimumScore) {
    return jsonResponse(
      {
        error: `Could not find a BEP that matches "${query}".`,
        suggestions: topMatches.map((item) => ({
          id: formatBepId(item.bep.number),
          title: item.bep.title,
        })),
      },
      404
    );
  }

  let exportDataRaw: unknown;
  try {
    exportDataRaw = await convex.query(api.export.getFullBepForExport, {
      bepId: bestMatch.bep._id as Id<"beps">,
    });
  } catch (err) {
    return jsonResponse(
      {
        error: "Failed to fetch BEP export data.",
        detail: err instanceof Error ? err.message : String(err),
      },
      502
    );
  }

  if (!exportDataRaw) {
    return jsonResponse({ error: "Matched BEP was not found." }, 404);
  }

  if (!isExportData(exportDataRaw)) {
    return jsonResponse({ error: "Invalid BEP export payload shape." }, 502);
  }

  // TODO: Align getFullBepForExport's generated return type with ExportData.
  const exportData = exportDataRaw;
  let selectedFiles: ExportFile[];
  let markdown: string;
  try {
    const allFiles = generateAllExportFiles(exportData).filter((file) =>
      file.path.endsWith(".md")
    );
    selectedFiles = omitOtherVersions
      ? allFiles.filter((file) => !file.path.startsWith("history/"))
      : allFiles;
    markdown = flattenMarkdownForAgents(selectedFiles);
  } catch (err) {
    return jsonResponse(
      {
        error: "Invalid BEP export payload shape.",
        detail: err instanceof Error ? err.message : String(err),
      },
      502
    );
  }

  if (format === "markdown") {
    return new Response(markdown, {
      status: 200,
      headers: {
        ...CORS_HEADERS,
        "Cache-Control": "no-store",
        "Content-Type": "text/markdown; charset=utf-8",
      },
    });
  }

  return jsonResponse({
    mode: "bep",
    query: normalizedQuery,
    matched: {
      id: formatBepId(bestMatch.bep.number),
      number: bestMatch.bep.number,
      title: bestMatch.bep.title,
      status: bestMatch.bep.status,
      score: Number(bestMatch.score.toFixed(3)),
    },
    currentVersion: exportData.currentVersion,
    omitOtherVersions,
    markdown,
    files: selectedFiles.map((file) => ({
      path: file.path,
      content: file.content,
    })),
  });
}
