import { get_version } from "./native";

function ensureVersionCompatibility(generatedVersion: string, runtimeVersion: string) {
  try {
    const [genMajor, genMinor] = generatedVersion.split(".").slice(0, 2);
    const [runtimeMajor, runtimeMinor] = runtimeVersion.split(".").slice(0, 2);

    return genMajor === runtimeMajor && genMinor === runtimeMinor;
  } catch (error) {
    return false; // Error parsing versions, assume incompatible
  }
}

export function ThrowIfVersionMismatch(generatedVersion: string) {
  const runtimeVersion = get_version();
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
