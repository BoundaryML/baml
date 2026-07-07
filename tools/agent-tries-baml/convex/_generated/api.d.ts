/* eslint-disable */
/**
 * Generated `api` utility.
 *
 * THIS CODE IS AUTOMATICALLY GENERATED.
 *
 * To regenerate, run `npx convex dev`.
 * @module
 */

import type * as bamlBuilds from "../bamlBuilds.js";
import type * as changelogEntries from "../changelogEntries.js";
import type * as cohorts from "../cohorts.js";
import type * as crons from "../crons.js";
import type * as issues from "../issues.js";
import type * as lib from "../lib.js";
import type * as maintenance from "../maintenance.js";
import type * as promoCodes from "../promoCodes.js";
import type * as tasks from "../tasks.js";
import type * as transcriptComments from "../transcriptComments.js";
import type * as trophies from "../trophies.js";
import type * as workers from "../workers.js";

import type {
  ApiFromModules,
  FilterApi,
  FunctionReference,
} from "convex/server";

declare const fullApi: ApiFromModules<{
  bamlBuilds: typeof bamlBuilds;
  changelogEntries: typeof changelogEntries;
  cohorts: typeof cohorts;
  crons: typeof crons;
  issues: typeof issues;
  lib: typeof lib;
  maintenance: typeof maintenance;
  promoCodes: typeof promoCodes;
  tasks: typeof tasks;
  transcriptComments: typeof transcriptComments;
  trophies: typeof trophies;
  workers: typeof workers;
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
