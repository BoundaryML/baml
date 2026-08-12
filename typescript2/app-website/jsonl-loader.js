// jsonl-loader.js

module.exports = function (source) {
  const callback = this.async();
  const lines = source.split('\n').filter((line) => line.trim() !== '');
  const parsedLines = lines
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch (error) {
        this.emitError(new Error(`Invalid JSON in line: ${line}`));
        return null;
      }
    })
    .filter((line) => line !== null);

  const result = JSON.stringify(parsedLines);
  callback(null, `export default ${result};`);
};
