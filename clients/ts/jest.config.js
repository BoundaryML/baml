// jest.config.js

module.exports = {
  preset: "ts-jest",
  testEnvironment: "node",
  verbose: true,
  globals: {
    NODE_ENV: "test",
  },
  moduleFileExtensions: ["js", "jsx", "ts", "tsx"],
  moduleDirectories: ["node_modules", "src"],
};
