// Test aids of the native bridge, kept off the package root so that the root
// exports the names today's bridge exports.
import "./index.js";

export const _native = globalThis[Symbol.for("baml.native.testing")];
