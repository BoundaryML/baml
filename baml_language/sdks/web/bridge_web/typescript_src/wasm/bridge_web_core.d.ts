export default function init(moduleOrPath?: unknown): Promise<unknown>;
export function stageRuntimeBytecode(bytecode: Uint8Array): void;
export function callFunction(functionName: string, encodedArgs: Uint8Array): Uint8Array;
export function newFunctionCall(): bigint;
