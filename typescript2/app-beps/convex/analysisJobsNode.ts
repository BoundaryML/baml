"use node";

import { v } from "convex/values";
import { internalAction } from "./_generated/server";
import { internal } from "./_generated/api";

/**
 * Run the BAML analysis in the background
 * This action runs in a Node.js environment to support the BAML native module
 */
export const runAnalysis = internalAction({
  args: { jobId: v.id("versionAnalysisJobs") },
  handler: async (ctx, args) => {
    try {
      // Validate environment
      if (!process.env.ANTHROPIC_API_KEY) {
        throw new Error("ANTHROPIC_API_KEY environment variable is not set");
      }

      // Update status to analyzing
      await ctx.runMutation(internal.analysisJobs.updateStatus, {
        jobId: args.jobId,
        status: "analyzing",
      });

      // Get analysis context
      const context = await ctx.runQuery(internal.analysisJobs.getAnalysisContext, {
        jobId: args.jobId,
      });

      // Import BAML client dynamically
      const { b } = await import("../baml_client");

      // Build input for BAML function
      const input = {
        bepNumber: context.bep.number,
        bepTitle: context.bep.title,
        previousVersion: {
          version: context.previousVersion.version,
          title: context.previousVersion.title,
          content: context.previousVersion.content,
          editedBy: context.previousVersion.editedBy,
          editNote: context.previousVersion.editNote ?? null,
        },
        newVersion: {
          version: context.newVersion.version,
          title: context.newVersion.title,
          content: context.newVersion.content,
          editedBy: context.newVersion.editedBy,
          editNote: context.newVersion.editNote ?? null,
        },
        feedbackItems: [
          ...context.comments.map(c => ({
            id: String(c.id),
            type: c.type,
            content: c.content,
            author: c.author,
            resolved: c.resolved,
            resolution: c.resolution ?? null,
          })),
          ...context.issues.map(i => ({
            id: String(i.id),
            type: i.type,
            content: i.content,
            author: i.author,
            resolved: i.resolved,
            resolution: i.resolution ?? null,
          })),
          ...context.decisions.map(d => ({
            id: String(d.id),
            type: d.type,
            content: d.content,
            author: d.author,
            resolved: d.resolved,
            resolution: d.resolution ?? null,
          })),
        ],
      };

      // Skip analysis if no feedback to analyze
      if (input.feedbackItems.length === 0) {
        await ctx.runMutation(internal.analysisJobs.updateStatus, {
          jobId: args.jobId,
          status: "completed",
          result: {
            addressedFeedback: [],
            unaddressedFeedback: [],
            partiallyAddressedFeedback: [],
            overallVerdict: "Good",
            verdictExplanation: "No feedback items to analyze for this version.",
            recommendations: [],
            summary: "This version has no prior feedback to address. The analysis is complete.",
          },
        });
        return;
      }

      // Early exit if versions have no content to compare
      if (!context.previousVersion.content && !context.newVersion.content) {
        await ctx.runMutation(internal.analysisJobs.updateStatus, {
          jobId: args.jobId,
          status: "completed",
          result: {
            addressedFeedback: [],
            unaddressedFeedback: [],
            partiallyAddressedFeedback: [],
            overallVerdict: "Good",
            verdictExplanation: "Both versions have no content to compare.",
            recommendations: [],
            summary: "No content available for analysis.",
          },
        });
        return;
      }

      // Call BAML function
      const result = await b.AnalyzeVersionChanges(input);

      // Update job with result
      await ctx.runMutation(internal.analysisJobs.updateStatus, {
        jobId: args.jobId,
        status: "completed",
        result: {
          addressedFeedback: result.addressedFeedback,
          unaddressedFeedback: result.unaddressedFeedback,
          partiallyAddressedFeedback: result.partiallyAddressedFeedback,
          overallVerdict: result.overallVerdict,
          verdictExplanation: result.verdictExplanation,
          recommendations: result.recommendations,
          summary: result.summary,
        },
      });

      // TODO: Notify peers via Slack when analysis completes
      // await ctx.scheduler.runAfter(0, internal.notifications.sendSlackAnalysisComplete, {
      //   jobId: args.jobId,
      //   verdict: result.overallVerdict,
      // });

    } catch (error) {
      // Update job with error
      await ctx.runMutation(internal.analysisJobs.updateStatus, {
        jobId: args.jobId,
        status: "failed",
        error: error instanceof Error ? error.message : "Unknown error",
      });
    }
  },
});
