import fs from "node:fs";
import path from "node:path";

const headersDir = path.resolve(process.cwd(), "..");
const watchedExtensions = new Set([".baml", ".snap"]);
const watchedSuffixes = [".snap.new"];

export const runtime = "nodejs";

export async function GET() {
  const encoder = new TextEncoder();

  const stream = new ReadableStream<Uint8Array>({
    start(controller) {
      const send = (payload: string) => {
        controller.enqueue(encoder.encode(`data: ${payload}\n\n`));
      };

      send("ready");

      const watcher = fs.watch(headersDir, (eventType, filename) => {
        if (!filename) {
          return;
        }

        const ext = path.extname(filename);
        const matchesExtension = watchedExtensions.has(ext);
        const matchesSuffix = watchedSuffixes.some((suffix) => filename.endsWith(suffix));
        if (!matchesExtension && !matchesSuffix) {
          return;
        }

        send(`${eventType}:${filename}`);
      });

      watcher.on("error", (error) => {
        send(`error:${error.message}`);
      });

      return () => {
        watcher.close();
      };
    },
  });

  return new Response(stream, {
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache, no-transform",
      Connection: "keep-alive",
    },
  });
}
