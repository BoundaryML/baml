var dedupe = require('../');
dedupe.activate();

var foo1 = require('./pack1/common/dep-uno/foo');
var foo2 = require('./pack2/common/dep-uno/foo');

console.log(foo1.foo);
console.log(foo2.foo);

console.log(foo1 === foo2);
