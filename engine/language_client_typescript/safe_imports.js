"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.ThrowIfVersionMismatch = void 0;
const native_1 = require("./native");
function ensureVersionCompatibility(generatedVersion, runtimeVersion) {
    try {
        const [genMajor, genMinor] = generatedVersion.split(".").slice(0, 2);
        const [runtimeMajor, runtimeMinor] = runtimeVersion.split(".").slice(0, 2);
        return genMajor === runtimeMajor && genMinor === runtimeMinor;
    }
    catch (error) {
        return false; // Error parsing versions, assume incompatible
    }
}
function ThrowIfVersionMismatch(generatedVersion) {
    const runtimeVersion = (0, native_1.get_version)();
    if (!ensureVersionCompatibility(generatedVersion, runtimeVersion)) {
        const errorMessage = `Update to @boundaryml/baml required.
Version from generators.baml: ${generatedVersion}
Current @boundaryml/baml version: ${runtimeVersion}

Please upgrade @boundaryml/baml to version ${generatedVersion}.

$ npm install @boundaryml/baml@${generatedVersion}
$ yarn add @boundaryml/baml@${generatedVersion}
$ pnpm add @boundaryml/baml@${generatedVersion}

If nothing else works, please ask for help:

https://github.com/boundaryml/baml/issues
https://boundaryml.com/discord`;
        throw new Error(errorMessage.trim());
    }
}
exports.ThrowIfVersionMismatch = ThrowIfVersionMismatch;
