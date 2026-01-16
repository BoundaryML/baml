/* eslint-disable */
/**
 * Generated `api` utility.
 *
 * THIS CODE IS AUTOMATICALLY GENERATED.
 *
 * To regenerate, run `npx convex dev`.
 * @module
 */

import type * as analysisJobs from "../analysisJobs.js";
import type * as analysisJobsNode from "../analysisJobsNode.js";
import type * as beps from "../beps.js";
import type * as comments from "../comments.js";
import type * as decisions from "../decisions.js";
import type * as export_ from "../export.js";
import type * as http from "../http.js";
import type * as issues from "../issues.js";
import type * as lib_prompts from "../lib/prompts.js";
import type * as migrations from "../migrations.js";
import type * as presence from "../presence.js";
import type * as users from "../users.js";

import type {
  ApiFromModules,
  FilterApi,
  FunctionReference,
} from "convex/server";

declare const fullApi: ApiFromModules<{
  analysisJobs: typeof analysisJobs;
  analysisJobsNode: typeof analysisJobsNode;
  beps: typeof beps;
  comments: typeof comments;
  decisions: typeof decisions;
  export: typeof export_;
  http: typeof http;
  issues: typeof issues;
  "lib/prompts": typeof lib_prompts;
  migrations: typeof migrations;
  presence: typeof presence;
  users: typeof users;
}>;

/**
 * A utility for referencing Convex functions in your app's public API.
 *
 * Usage:
 * ```js
 * const myFunctionReference = api.myModule.myFunction;
 * ```
 */
export declare const api: FilterApi<
  typeof fullApi,
  FunctionReference<any, "public">
>;

/**
 * A utility for referencing Convex functions in your app's internal API.
 *
 * Usage:
 * ```js
 * const myFunctionReference = internal.myModule.myFunction;
 * ```
 */
export declare const internal: FilterApi<
  typeof fullApi,
  FunctionReference<any, "internal">
>;

export declare const components: {};
