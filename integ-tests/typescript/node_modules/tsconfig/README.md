# TSConfig

[![NPM version][npm-image]][npm-url]
[![NPM downloads][downloads-image]][downloads-url]
[![Build status][travis-image]][travis-url]
[![Test coverage][coveralls-image]][coveralls-url]

Resolve and parse [`tsconfig.json`][tsconfig], replicating TypeScript's behaviour.

## Usage

See the [TypeScript docs][tsconfig] for information on setting up `tsconfig.json`.

### API

* **resolve(cwd: string, path?: string): Promise<string | void>** Resolve `tsconfig.json`, like TypeScript, allowing a path to be specified and falling back to recursively resolving `tsconfig.json` upward when no path is specified.
* **resolveSync(cwd: string, path?: string): string | void** Synchronous `resolve`.
* **find(cwd: string): Promise<string | void>** Standalone behaviour of recursively resolving `tsconfig.json` upward.
* **findSync(cwd: string): string | void** Synchronous `find`.
* **load(cwd: string, path?: string): Promise<{ path?: string, config: any }>** Resolve, load and parse `tsconfig.json`.
* **loadSync(cwd: string, path?: string): { path?: string, config: any }** Synchronous `load`.
* **readFile(filename: string): Promise<any>** Read a JSON file as `tsconfig.json` (strip BOM, parse JSON and support empty contents).
* **readFileSync(filename: string): any** Synchronous `readFile`.
* **parse(contents: string, filename: string): any** Parse file contents as `tsconfig.json` (strip BOM, parse JSON and support empty contents).

## Contributing

Feel free to open issues for discussion.

```sh
# Install dependencies/typings.
npm install

# Run test suite.
npm test
```

## License

MIT License

[npm-image]: https://img.shields.io/npm/v/tsconfig.svg?style=flat
[npm-url]: https://npmjs.org/package/tsconfig
[downloads-image]: https://img.shields.io/npm/dm/tsconfig.svg?style=flat
[downloads-url]: https://npmjs.org/package/tsconfig
[travis-image]: https://img.shields.io/travis/TypeStrong/tsconfig.svg?style=flat
[travis-url]: https://travis-ci.org/TypeStrong/tsconfig
[coveralls-image]: https://img.shields.io/coveralls/TypeStrong/tsconfig.svg?style=flat
[coveralls-url]: https://coveralls.io/r/TypeStrong/tsconfig?branch=master
[tsconfig]: http://www.typescriptlang.org/docs/handbook/tsconfig-json.html
