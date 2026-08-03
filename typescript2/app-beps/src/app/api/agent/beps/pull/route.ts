import { NextRequest, NextResponse } from "next/server";
import { ConvexHttpClient } from "convex/browser";
import { api } from "../../../../../../convex/_generated/api";
import {
  type ExportAllData,
  generateAllBepsExportFiles,
} from "@/lib/export-all-utils";
import JSZip from "jszip";

export const runtime = "nodejs";
export const dynamic = "force-dynamic";

const CORS_HEADERS = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Methods": "GET, OPTIONS",
  "Access-Control-Allow-Headers": "Content-Type",
};

export async function OPTIONS(): Promise<Response> {
  return new Response(null, {
    status: 204,
    headers: CORS_HEADERS,
  });
}

export async function GET(request: NextRequest): Promise<Response> {
  const convexUrl = process.env.NEXT_PUBLIC_CONVEX_URL;
  if (!convexUrl) {
    return NextResponse.json(
      { error: "Missing NEXT_PUBLIC_CONVEX_URL environment variable." },
      { status: 500, headers: CORS_HEADERS }
    );
  }

  const convex = new ConvexHttpClient(convexUrl);

  let exportData: ExportAllData;
  try {
    const rawData = await convex.query(api.export.getAllBepsForExport, {});
    exportData = rawData as unknown as ExportAllData;
  } catch (err) {
    return NextResponse.json(
      {
        error: "Failed to fetch BEP data.",
        detail: err instanceof Error ? err.message : String(err),
      },
      { status: 502, headers: CORS_HEADERS }
    );
  }

  const apiBaseUrl = request.nextUrl.origin;
  const files = generateAllBepsExportFiles(exportData, apiBaseUrl);

  const zip = new JSZip();
  const folderName = "all-beps";
  const folder = zip.folder(folderName);

  if (!folder) {
    return NextResponse.json(
      { error: "Failed to create ZIP folder." },
      { status: 500, headers: CORS_HEADERS }
    );
  }

  for (const file of files) {
    const fileOptions = file.unixPermissions
      ? { unixPermissions: file.unixPermissions }
      : undefined;
    const parts = file.path.split("/");
    if (parts.length > 1) {
      const folderPath = parts.slice(0, -1).join("/");
      const fileName = parts[parts.length - 1];
      const nestedFolder = folder.folder(folderPath);
      if (nestedFolder) {
        nestedFolder.file(fileName, file.content, fileOptions);
      }
    } else {
      folder.file(file.path, file.content, fileOptions);
    }
  }

  const zipContent = await zip.generateAsync({
    type: "arraybuffer",
    platform: "UNIX",
  });

  const date = new Date().toISOString().split("T")[0];
  const filename = `all-beps-${date}.zip`;

  return new Response(zipContent, {
    status: 200,
    headers: {
      ...CORS_HEADERS,
      "Content-Type": "application/zip",
      "Content-Disposition": `attachment; filename="${filename}"`,
      "Cache-Control": "no-store",
    },
  });
}
