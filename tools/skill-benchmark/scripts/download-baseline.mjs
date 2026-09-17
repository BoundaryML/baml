import { appendFile, writeFile } from "node:fs/promises";

const token = process.env.GITHUB_TOKEN;
const repository = process.env.GITHUB_REPOSITORY;
const output = process.env.GITHUB_OUTPUT;
const destination = process.env.BASELINE_ARCHIVE ?? "baseline.zip";

if (!token || !repository || !output) {
  throw new Error("GITHUB_TOKEN, GITHUB_REPOSITORY, and GITHUB_OUTPUT are required");
}

const headers = {
  Accept: "application/vnd.github+json",
  Authorization: `Bearer ${token}`,
  "X-GitHub-Api-Version": "2022-11-28",
};

async function github(path) {
  const response = await fetch(`https://api.github.com/repos/${repository}${path}`, { headers });
  if (!response.ok) {
    if (response.status === 404) {
      return null;
    }
    throw new Error(`${response.status} ${response.statusText}: ${await response.text()}`);
  }
  return response;
}

const runsResponse = await github(
  "/actions/workflows/skill-benchmark.yml/runs?branch=canary&status=success&per_page=25",
);
let selected = null;

if (runsResponse) {
  const { workflow_runs: runs } = await runsResponse.json();
  for (const run of runs) {
    const artifactsResponse = await github(`/actions/runs/${run.id}/artifacts?per_page=100`);
    if (!artifactsResponse) {
      continue;
    }
    const { artifacts } = await artifactsResponse.json();
    selected = artifacts.find(
      (artifact) => artifact.name === "skill-benchmark-baseline" && !artifact.expired,
    );
    if (selected) {
      break;
    }
  }
}

if (!selected) {
  await appendFile(output, "found=false\n");
  process.exit(0);
}

const archive = await github(`/actions/artifacts/${selected.id}/zip`);
if (!archive) {
  throw new Error(`baseline artifact ${selected.id} disappeared before download`);
}
await writeFile(destination, Buffer.from(await archive.arrayBuffer()));
await appendFile(output, `found=true\nartifact_id=${selected.id}\n`);
