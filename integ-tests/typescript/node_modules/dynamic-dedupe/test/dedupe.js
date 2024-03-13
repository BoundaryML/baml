'use strict';
/*jshint asi: true */

var test = require('tap').test
var dedupe = require('../');
var count = require('./fixtures/count');

function reset() {
  var files = [
    './fixtures/pack1/common/dep-uno/foo'
  , './fixtures/pack1/common/dep-uno/bar'
  , './fixtures/pack1/common/dep-dos/foo'
  , './fixtures/pack2/common/dep-uno/foo'
  , './fixtures/pack2/common/dep-uno/bar'
  ].map(require.resolve);

  files.forEach(function (k) { delete require.cache[k] });

  dedupe.deactivate();
  dedupe.reset();
  count.count = 0;
}

test('\nactive: when I require pack1/common/dep-uno/foo and pack2/common/dep-uno/foo', function (t) {
  reset()
  
  dedupe.activate('.js');
  var foo1 = require('./fixtures/pack1/common/dep-uno/foo');
  var foo2 = require('./fixtures/pack2/common/dep-uno/foo');
  
  t.equal(count.count, 1, 'loads it only once')
  t.equal(foo1.foo, 'foobiloo', 'returns exports 1')
  t.equal(foo2.foo, 'foobiloo', 'returns exports 2')
  t.end()
})

test('\nactive: when I require pack1/common/dep-dos/foo and pack2/common/dep-uno/foo', function (t) {
  reset()
  
  dedupe.activate('.js');
  var foo1 = require('./fixtures/pack1/common/dep-dos/foo');
  var foo2 = require('./fixtures/pack2/common/dep-uno/foo');
  
  t.equal(count.count, 2, 'loads it twice')
  t.equal(foo1.foo, 'foobiloo', 'returns exports 1')
  t.equal(foo2.foo, 'foobiloo', 'returns exports 2')
  t.end()
})

test('\nactive: when I require pack1/common/dep-uno/foo and pack1/common/dep-uno/bar', function (t) {
  reset()
  
  dedupe.activate('.js');
  var foo = require('./fixtures/pack1/common/dep-uno/foo');
  var bar = require('./fixtures/pack1/common/dep-uno/bar');
  
  t.equal(count.count, 2, 'loads it twice')
  t.equal(foo.foo, 'foobiloo', 'returns exports 1')
  t.equal(bar.foo, 'foobiloo', 'returns exports 2')
  t.end()
})

test('\nactive: when I require pack1/common/dep-uno/bar and pack2/common/dep-uno/bar', function (t) {
  reset()
  
  dedupe.activate('.js');
  var bar1 = require('./fixtures/pack1/common/dep-uno/bar');
  var bar2 = require('./fixtures/pack2/common/dep-uno/bar');
  
  t.equal(count.count, 1, 'loads it only once')
  t.equal(bar1.foo, 'foobiloo', 'returns exports 1')
  t.equal(bar2.foo, 'foobiloo', 'returns exports 2')
  t.end()
})

test('\nactive then deactivated: when I require pack1/common/dep-uno/foo and pack2/common/dep-uno/foo', function (t) {
  reset()
  
  dedupe.activate('.js');
  var foo1 = require('./fixtures/pack1/common/dep-uno/foo');
  dedupe.deactivate('.js');
  var foo2 = require('./fixtures/pack2/common/dep-uno/foo');
  
  t.equal(count.count, 2, 'loads it twice')
  t.equal(foo1.foo, 'foobiloo', 'returns exports 1')
  t.equal(foo2.foo, 'foobiloo', 'returns exports 2')
  t.end()
})

test('\nactive: subdir 3, when I require pack1/common/dep-uno/foo and pack2/common/dep-uno/foo', function (t) {
  reset()
  
  dedupe.activate('.js', 3);
  var foo1 = require('./fixtures/pack1/common/dep-uno/foo');
  var foo2 = require('./fixtures/pack2/common/dep-uno/foo');
  
  t.equal(count.count, 2, 'loads it twice since only two subdirs match')
  t.equal(foo1.foo, 'foobiloo', 'returns exports 1')
  t.equal(foo2.foo, 'foobiloo', 'returns exports 2')
  t.end()
})
