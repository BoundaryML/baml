# dynamic-dedupe

Dedupes node modules as they are being required  which works even when dependencies are linked via ln -s or npm link.

### Not deduped 

Loads `foo.js` module only twice.

```js
var foo1 = require('./pack1/common/dep-uno/foo');
var foo2 = require('./pack2/common/dep-uno/foo');

console.log(foo1.foo);
console.log(foo2.foo);

console.log(foo1 === foo2);

// =>
// loading foo from /Users/thlorenz/dev/projects/dynamic-dedupe/example/pack1/common/dep-uno
// loading foo from /Users/thlorenz/dev/projects/dynamic-dedupe/example/pack2/common/dep-uno
// foobiloo
// foobiloo
// false
```

### Deduped

Loads `foo.js` module only once.

```js
var dedupe = require('../');
dedupe.activate();

var foo1 = require('./pack1/dep-uno/foo');
var foo2 = require('./pack2/dep-uno/foo');

console.log(foo1.foo);
console.log(foo2.foo);

console.log(foo1 === foo2);

// =>
// loading foo from /Users/thlorenz/dev/projects/dynamic-dedupe/example/pack1/common/dep-uno
// foobiloo
// foobiloo
// true
```

Here instead of loading `pack2/dep-uno/foo1.js` we will get a reference to the exports of `pack1/dep-uno/foo`.js`
returned.


## Why?

In some cases an app may be split into multiple parts that need to get the same instance of a common dependency (i.e.
Handlebars). This will work once you run `npm dedupe` from the main package. However once you try linking to a
dependency via `npm link` or just `ln -s` it breaks.

This is where dynamic-dedupe comes in since it dedupes your modules as they are being required. Just **make sure that
you are using the exact same version** of the packages whose modules you dedupe in order for this to work reliably.

## Installation

    npm install dynamic-dedupe

## API

###*dedupe.activate([ext, subdirs])*

```
/**
 * Activates deduping for files with the given extension.
 * 
 * @name activate
 * @function
 * @param ext {String} (optional) extension for which to activate deduping (default: '.js')
 * @param subdirs {Number} (optional) how many subdirs right above the module
 *    have to be the same in order for it to be considered identical  (default: 2)
 *
 *  Example: sudirs: 2 -- x/foo/bar/main.js === y/foo/bar/main.js
 *                        x/boo/bar/main.js !== y/foo/bar/main.js
 */
```

###*dedupe.deactivate([ext])*

```
/**
 * Deactivates deduping files with the given extension.
 * 
 * @name deactivate
 * @function
 * @param ext {String} (optional) extension for which to activate deduping (default: '.js')
 */
```

###*dedupe.reset()*

```
/**
 * Clears the registry that contains previously loaded modules.
 * 
 * @name reset
 * @function
 */
```

## License

MIT
