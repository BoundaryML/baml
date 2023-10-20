/* tslint:disable */
/* eslint-disable */
/**
* Docs: https://prisma.github.io/prisma-engines/doc/prisma_fmt/fn.get_config.html
* @param {string} input
* @returns {string}
*/
export function lint(input: string): string;
/**
* @param {string} params
*/
export function validate(params: string): void;
/**
* The API is modelled on an LSP [completion
* request](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/specification-3-16.md#textDocument_completion).
* Input and output are both JSON, the request being a `CompletionParams` object and the response
* being a `CompletionList` object.
* This API is modelled on an LSP [code action
* request](https://github.com/microsoft/language-server-protocol/blob/gh-pages/_specifications/specification-3-16.md#textDocument_codeAction=).
* Input and output are both JSON, the request being a
* `CodeActionParams` object and the response being a list of
* `CodeActionOrCommand` objects.
* Trigger a panic inside the wasm module. This is only useful in development for testing panic
* handling.
*/
export function debug_panic(): void;
