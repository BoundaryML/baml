class Hello { constructor(message) { this.message = message; } }
function setup() { return new Hello("Hello, World!"); }
function run(hello) { return JSON.stringify(hello); }
function main() { return run(setup()); }
