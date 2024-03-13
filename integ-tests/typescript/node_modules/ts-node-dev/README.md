# ts-node-dev

> Tweaked version of [node-dev](https://github.com/fgnass/node-dev) that uses [ts-node](https://github.com/TypeStrong/ts-node) under the hood.

It restarts target node process when any of required files changes (as standard `node-dev`) but shares [Typescript](https://github.com/Microsoft/TypeScript/) compilation process between restarts. This significantly increases speed of restarting comparing to `node-dev -r ts-node/register ...`, `nodemon -x ts-node ...` variations because there is no need to instantiate `ts-node` compilation each time.

## Install

![npm (scoped)](https://img.shields.io/npm/v/ts-node-dev.svg?maxAge=86400) [![Build Status](https://travis-ci.com/wclr/ts-node-dev.svg?branch=master)](https://travis-ci.com/wclr/ts-node-dev)

```
yarn add ts-node-dev --dev
```

```
npm i ts-node-dev --save-dev
```

## Usage

```
ts-node-dev [node-dev|ts-node flags] [ts-node-dev flags] [node cli flags] [--] [script] [script arguments]
```

So you just combine [node-dev](https://github.com/fgnass/node-dev) and [ts-node](https://github.com/TypeStrong/ts-node) options (see docs of those packages):

```
ts-node-dev --respawn --transpile-only server.ts
```

There is also short alias `tsnd` for running `ts-node-dev`:

```
tsnd --respawn server.ts
```

Look up flags and options can be used [in ts-node's docs](https://github.com/TypeStrong/ts-node#cli-and-programmatic-options).

**Also there are additional options specific to `ts-node-dev`:**

* `--ignore-watch` - (default: []) - files/folders to be [ignored by `node-dev`](https://github.com/fgnass/node-dev#ignore-paths). **But this behaviour is enhanced:** it also supports regular expression in the ignore strings and will check absolute paths of required files for match.

* `--deps` - Also watch `node_modules`; by default watching is turned off

* `--debug` - Some additional [DEBUG] output
* `--quiet` - Silent [INFO] messages
* `--interval` - Polling interval (ms) - DOESN'T WORK CURRENTLY
* `--debounce` - Debounce file change events (ms, non-polling mode)
* `--clear` (`--cls`) - Will clear screen on restart
* `--watch` - Explicitly add arbitrary files or folders to watch and restart on change (list separated by commas, [chokidar](https://github.com/paulmillr/chokidar) patterns)
* `--exit-child` - Adds 'SIGTERM' exit handler in a child process.
* `--rs` - Allow to restart with "rs" line entered in stdio, disabled by default.
* `--notify` - to display desktop-notifications (Notifications are only displayed if `node-notifier` is installed).
* `--cache-directory` - tmp dir which is used to keep the compiled sources (by default os tmp directory is used)

If you need to detect that you are running with `ts-node-dev`, check if `process.env.TS_NODE_DEV` is set.


**Points of notice:**

- If you want desktop-notifications you should install `node-notifier` package and use `--notify` flag.

- Especially for large code bases always consider running with `--transpile-only` flag which is normal for dev workflow and will speed up things greatly. Note, that `ts-node-dev` will not put watch handlers on TS files that contain only types/interfaces (used only for type checking) - this is current limitation by design.

- `--ignore-watch` will NOT affect files ignored by TS compilation. Use `--ignore` option (or `TS_NODE_IGNORE` env variable) to pass **RegExp strings** for filtering files that should not be compiled, by default `/node_modules/` are ignored.

- Unknown flags (`node` cli flags are considered to be so) are treated like string value flags by default. The right solution to avoid ambiguity is to separate script name from option flags with `--`, for example:

  ```
  ts-node-dev --inspect -- my-script.ts
  ```

- The good thing is that `ts-node-dev` watches used `tsconfig.json` file, and will reinitialize compilation on its change, but you have to restart the process manually when you update used version of `typescript` or make any other changes that may effect compilation results.

## Issues

If you have an issue, please create one. But, before:
- try to check if there exits alike issues.
- try to run your code with just [ts-node](https://github.com/TypeStrong/ts-node)
- try to run your code with `--files` option enabled ([see ts-node docs](https://github.com/TypeStrong/ts-node#help-my-types-are-missing))
- try to run it with `--debug` flag and see the output
- try to make create repro example

## Versioning

Currently versioning is not stable and it is still treated as pre-release. You might expect some options API changes. If you want to avoid unexpected problems it is recommended to fixate the installed version and update only in case of issues, you may consult [CHANGELOG](CHANGELOG.md) for updates.

## License

MIT.
