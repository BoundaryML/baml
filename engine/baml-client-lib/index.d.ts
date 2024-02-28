// index.d.ts


// Initialize the tracer, might require specific arguments depending on your implementation.
export function initTracer(): void;


// Set tags for the current trace.
// The tags are key-value pairs, where the key is a string and the value is a string or null. When the value is null, the tag is removed if it was previously set in the trace tree.
export function setTags(tags: {
  [key: string]: string | null;
}): void;

// Define a more specific type for the argument metadata.
type ArgMetadata = { name: string; type: string };

// Use a generic type for the trace function to ensure type safety on the callback function and its return type.
// TArgs is a tuple representing the arguments of the callback function, and TReturn is the return type of the callback function.
export function trace<TArgs extends any[], TReturn>(
  cb: (...args: TArgs) => TReturn,
  name: string,
  args: ArgMetadata[],
  asKwargs: boolean,
  returnType: string
): (...args: TArgs) => TReturn;

export function traceAsync<TArgs extends any[], TReturn>(
  cb: (...args: TArgs) => Promise<TReturn>,
  name: string,
  args: ArgMetadata[],
  asKwargs: boolean,
  returnType: string
): (...args: TArgs) => Promise<TReturn>;
