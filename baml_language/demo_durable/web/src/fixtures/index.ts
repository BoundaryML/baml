import type { Fixture } from "./builder";
import { buildBranchFixture } from "./branch";
import { buildCentralFixture } from "./central";
import { buildChainFixture } from "./chain";
import { buildDeadlineFixture } from "./deadline";
import { buildFanoutFixture } from "./fanout";
import { buildForkFixture } from "./fork";
import { buildPoolFixture } from "./pool";
import { buildRaceFixture } from "./race";
import { buildRecoverFixture } from "./recover";
import { buildSettledFixture } from "./settled";
import { buildSpawnFixture } from "./spawn";

export type { Fixture } from "./builder";

export const FIXTURE_NAMES = ["central", "pool", "chain", "spawn", "fork", "fanout", "race", "deadline", "settled", "recover", "branch"] as const;
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
    case "fanout":
      return buildFanoutFixture();
    case "race":
      return buildRaceFixture();
    case "deadline":
      return buildDeadlineFixture();
    case "settled":
      return buildSettledFixture();
    case "recover":
      return buildRecoverFixture();
    case "branch":
      return buildBranchFixture();
  }
}
