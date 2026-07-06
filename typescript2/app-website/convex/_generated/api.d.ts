/* eslint-disable */
/**
 * Generated `api` utility.
 *
 * THIS CODE IS AUTOMATICALLY GENERATED.
 *
 * To regenerate, run `npx convex dev`.
 * @module
 */

import type * as bamlcodeFeedback from "../bamlcodeFeedback.js";
import type * as council from "../council.js";
import type * as discord from "../discord.js";
import type * as loops from "../loops.js";
import type * as slopPledges from "../slopPledges.js";
import type * as triggers from "../triggers.js";

import type {
  ApiFromModules,
  FilterApi,
  FunctionReference,
} from "convex/server";

declare const fullApi: ApiFromModules<{
  bamlcodeFeedback: typeof bamlcodeFeedback;
  council: typeof council;
  discord: typeof discord;
  loops: typeof loops;
  slopPledges: typeof slopPledges;
  triggers: typeof triggers;
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
