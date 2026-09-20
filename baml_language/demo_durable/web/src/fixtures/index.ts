import type { Fixture } from "./builder";
import { buildCentralFixture } from "./central";
import { buildChainFixture } from "./chain";
import { buildForkFixture } from "./fork";
import { buildPoolFixture } from "./pool";
import { buildSpawnFixture } from "./spawn";

export type { Fixture } from "./builder";

export const FIXTURE_NAMES = ["central", "pool", "chain", "spawn", "fork"] as const;
export type FixtureName = (typeof FIXTURE_NAMES)[number];

export function isFixtureName(value: string | null): value is FixtureName {
  return value !== null && (FIXTURE_NAMES as readonly string[]).includes(value);
}

export function loadFixture(name: FixtureName): Fixture {
  switch (name) {
    case "central":
      return buildCentralFixture();
    case "pool":
      return buildPoolFixture();
    case "chain":
      return buildChainFixture();
    case "spawn":
      return buildSpawnFixture();
    case "fork":
      return buildForkFixture();
  }
}
