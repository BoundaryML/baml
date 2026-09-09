function acceptsScript(source) {
  try {
    new Function(source);
    return true;
  } catch (error) {
    return `${error.name}: ${error.message}`;
  }
}

console.log(`node=${process.version}`);
console.log(`v8=${process.versions.v8}`);
console.log(`Symbol.dispose=${typeof Symbol.dispose}`);
console.log(`Symbol.asyncDispose=${typeof Symbol.asyncDispose}`);
console.log(`raw using=${String(acceptsScript("using resource = {};"))}`);
console.log(
  `raw await using=${String(acceptsScript("async function probe() { await using resource = {}; }"))}`,
);
