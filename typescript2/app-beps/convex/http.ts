import { httpRouter } from "convex/server";
import { httpAction } from "./_generated/server";
import { internal } from "./_generated/api";
import Anthropic from "@anthropic-ai/sdk";
import {
  buildAIAssistantPrompt,
  type BepContext,
  type CommentContext,
  type AIAssistantContext,
  type VersionContext,
  type IssueContext,
  type DecisionContext,
  type QuickAction,
} from "./lib/prompts";
import { Id } from "./_generated/dataModel";

const http = httpRouter();

const MODEL = "claude-sonnet-4-20250514";

// ─────────────────────────────────────────────────────────────────────────────
// Streaming AI Assistant Endpoint (Version Comparison & Q&A)
// ─────────────────────────────────────────────────────────────────────────────

http.route({
  path: "/api/ai/stream-assistant",
  method: "POST",
  handler: httpAction(async (ctx, request) => {
    try {
      const body = await request.json();
      const {
        bepId,
        fromVersionId,
        toVersionId,
        question,
        quickAction,
        conversationHistory,
      } = body as {
        bepId: Id<"beps">;
        fromVersionId?: Id<"bepVersions">;
        toVersionId?: Id<"bepVersions">;
        question: string;
        quickAction?: QuickAction;
        conversationHistory?: Array<{ role: "user" | "assistant"; content: string }>;
      };

      // Validate input
      if (!bepId) {
        return new Response(
          JSON.stringify({ error: "Missing required field: bepId" }),
          {
            status: 400,
            headers: { "Content-Type": "application/json" },
          }
        );
      }

      if (!question && !quickAction) {
        return new Response(
          JSON.stringify({ error: "Either question or quickAction is required" }),
          {
            status: 400,
            headers: { "Content-Type": "application/json" },
          }
        );
      }

      // Fetch BEP data
      const bep = await ctx.runQuery(internal.beps.getById, { id: bepId });
      if (!bep) {
        return new Response(JSON.stringify({ error: "BEP not found" }), {
          status: 404,
          headers: { "Content-Type": "application/json" },
        });
      }

      // Fetch all versions for the BEP to get the latest
      const allVersions = await ctx.runQuery(internal.beps.getVersionsByBep, { bepId });
      const latestVersion = allVersions.length > 0 ? allVersions[0] : null;

      // If toVersionId is provided, fetch that specific version, otherwise use latest
      let toVersion = latestVersion;
      if (toVersionId) {
        toVersion = await ctx.runQuery(internal.beps.getVersionById, { id: toVersionId });
        if (!toVersion) {
          return new Response(JSON.stringify({ error: "To version not found" }), {
            status: 404,
            headers: { "Content-Type": "application/json" },
          });
        }
      }

      // Fetch from version if provided (for comparison)
      let fromVersion = null;
      if (fromVersionId && fromVersionId !== toVersionId) {
        fromVersion = await ctx.runQuery(internal.beps.getVersionById, { id: fromVersionId });
        if (!fromVersion) {
          return new Response(JSON.stringify({ error: "From version not found" }), {
            status: 404,
            headers: { "Content-Type": "application/json" },
          });
        }
      }

      // Fetch comments - if we have a specific version, get those comments
      // Otherwise get all comments for the BEP
      let toVersionCommentsRaw: Awaited<ReturnType<typeof ctx.runQuery<typeof internal.comments.byVersion>>> = [];
      if (toVersion) {
        toVersionCommentsRaw = await ctx.runQuery(internal.comments.byVersion, {
          bepId,
          versionId: toVersion._id,
        });
      }

      let fromVersionCommentsRaw: typeof toVersionCommentsRaw = [];
      if (fromVersion) {
        fromVersionCommentsRaw = await ctx.runQuery(internal.comments.byVersion, {
          bepId,
          versionId: fromVersion._id,
        });
      }

      // Fetch issues and decisions
      const issues = await ctx.runQuery(internal.issues.byBepInternal, { bepId });
      const decisions = await ctx.runQuery(internal.decisions.byBepInternal, { bepId });

      // Build context objects
      const bepContext: BepContext = {
        number: bep.number,
        title: bep.title,
        status: bep.status,
        content: bep.content ?? "",
      };

      const toVersionContext: VersionContext | undefined = toVersion
        ? {
            version: toVersion.version,
            title: toVersion.title,
            content: toVersion.content ?? "",
            createdAt: toVersion.createdAt,
            editedBy: toVersion.editedByName,
            editNote: toVersion.editNote,
          }
        : undefined;

      const fromVersionContext: VersionContext | undefined = fromVersion
        ? {
            version: fromVersion.version,
            title: fromVersion.title,
            content: fromVersion.content ?? "",
            createdAt: fromVersion.createdAt,
            editedBy: fromVersion.editedByName,
            editNote: fromVersion.editNote,
          }
        : undefined;

      const formatCommentsForContext = (comments: typeof toVersionCommentsRaw): CommentContext[] =>
        comments.map((c) => ({
          id: c._id,
          type: c.type,
          content: c.content,
          authorName: c.authorName,
          createdAt: c.createdAt,
          resolved: c.resolved,
          parentId: c.parentId,
        }));

      const issueContexts: IssueContext[] = issues.map((i) => ({
        title: i.title,
        description: i.description,
        raisedBy: i.raisedByName,
        resolved: i.resolved,
        resolution: i.resolution,
      }));

      const decisionContexts: DecisionContext[] = decisions.map((d) => ({
        title: d.title,
        description: d.description,
        rationale: d.rationale,
        participants: d.participantNames,
        decidedAt: d.decidedAt,
      }));

      // Build the AI assistant context
      const aiContext: AIAssistantContext = {
        bep: bepContext,
        fromVersion: fromVersionContext,
        toVersion: toVersionContext!,
        fromVersionComments: formatCommentsForContext(fromVersionCommentsRaw),
        toVersionComments: formatCommentsForContext(toVersionCommentsRaw),
        issues: issueContexts,
        decisions: decisionContexts,
      };

      // Build the prompt
      const systemPrompt = buildAIAssistantPrompt(aiContext, question, quickAction);

      // Build messages array - include conversation history for follow-ups
      const messages: Array<{ role: "user" | "assistant"; content: string }> = [];

      if (conversationHistory && conversationHistory.length > 0) {
        // First message is the system context, then add conversation history
        messages.push({ role: "user", content: systemPrompt });
        messages.push(...conversationHistory);
        // The current question is the last user message
        if (question) {
          messages.push({ role: "user", content: question });
        }
      } else {
        // No history - just the initial prompt
        messages.push({ role: "user", content: systemPrompt });
      }

      // Create streaming response
      const anthropic = new Anthropic();

      const { readable, writable } = new TransformStream();
      const writer = writable.getWriter();
      const encoder = new TextEncoder();

      // Start streaming in the background
      (async () => {
        try {
          const stream = anthropic.messages.stream({
            model: MODEL,
            max_tokens: 4096,
            messages,
          });

          for await (const event of stream) {
            if (
              event.type === "content_block_delta" &&
              event.delta.type === "text_delta"
            ) {
              const text = event.delta.text;
              await writer.write(encoder.encode(text));
            }
          }

          // Send completion marker
          await writer.write(encoder.encode("\n\n---STREAM_COMPLETE---\n"));
        } catch (error) {
          console.error("AI Assistant streaming error:", error);
          await writer.write(
            encoder.encode(
              `\n\n---STREAM_ERROR---\n${error instanceof Error ? error.message : "Unknown error"}\n`
            )
          );
        } finally {
          await writer.close();
        }
      })();

      return new Response(readable, {
        headers: {
          "Content-Type": "text/plain; charset=utf-8",
          "Transfer-Encoding": "chunked",
          "Cache-Control": "no-cache",
          "Access-Control-Allow-Origin": "*",
          "Access-Control-Allow-Methods": "POST, OPTIONS",
          "Access-Control-Allow-Headers": "Content-Type",
        },
      });
    } catch (error) {
      console.error("AI Assistant HTTP action error:", error);
      return new Response(
        JSON.stringify({
          error: error instanceof Error ? error.message : "Internal error",
        }),
        {
          status: 500,
          headers: { "Content-Type": "application/json" },
        }
      );
    }
  }),
});

// Handle CORS preflight for AI assistant
http.route({
  path: "/api/ai/stream-assistant",
  method: "OPTIONS",
  handler: httpAction(async () => {
    return new Response(null, {
      status: 204,
      headers: {
        "Access-Control-Allow-Origin": "*",
        "Access-Control-Allow-Methods": "POST, OPTIONS",
        "Access-Control-Allow-Headers": "Content-Type",
        "Access-Control-Max-Age": "86400",
      },
    });
  }),
});

export default http;
