import { readdir, readFile, writeFile } from "node:fs/promises";
import { join } from "node:path";

const token = process.env.GITHUB_TOKEN;
const repository = process.env.GITHUB_REPOSITORY;
const runId = process.env.GITHUB_RUN_ID;
const reportsDirectory = process.env.ATTEMPT_REPORTS_DIRECTORY;
const outputPath = process.env.REPORT_LINKS_PATH;

if (!token || !repository || !runId || !reportsDirectory || !outputPath) {
  throw new Error("GitHub context and report path environment variables are required");
}

const headers = {
  Accept: "application/vnd.github+json",
  Authorization: `Bearer ${token}`,
  "X-GitHub-Api-Version": "2022-11-28",
};

async function github(path) {
  const response = await fetch(`https://api.github.com/repos/${repository}${path}`, { headers });
  if (!response.ok) {
    throw new Error(`${response.status} ${response.statusText}: ${await response.text()}`);
  }
  return response;
}

async function allJobs() {
  const jobs = [];
  for (let page = 1; ; page += 1) {
    const response = await github(`/actions/runs/${runId}/jobs?filter=latest&per_page=100&page=${page}`);
    const body = await response.json();
    jobs.push(...body.jobs);
    if (body.jobs.length < 100) {
      return jobs;
    }
  }
}

function evidenceLine(log, invocationId) {
  const lines = log.split("\n");
  const target = lines.findIndex((line) => line.includes(invocationId));
  if (target < 0) {
    return 1;
  }

  let group = target;
  while (group > 0 && !lines[group].includes("SKILL_BENCH_INDEX_BEGIN")) {
    group -= 1;
  }
  if (!lines[group].includes("SKILL_BENCH_INDEX_BEGIN")) {
    return 1;
  }
  return target - group + 1;
}

const reports = [];
for (const entry of await readdir(reportsDirectory, { recursive: true })) {
  if (entry.endsWith(".json")) {
    reports.push(JSON.parse(await readFile(join(reportsDirectory, entry), "utf8")));
  }
}

const jobs = await allJobs();
const links = { attempts: [], evidence: [] };

for (const report of reports) {
  const expectedName = `skill benchmark / ${report.project_name} / attempt ${report.attempt}`;
  const job = jobs.find((candidate) => candidate.name === expectedName);
  if (!job) {
    continue;
  }

  const artifactUrl = report.artifacts.transcript_url ?? job.html_url;
  links.attempts.push({
    attempt_id: report.id,
    job_url: job.html_url,
    artifact_url: artifactUrl,
  });

  const publishStep = job.steps.find((step) => step.name === "Publish indexed agent log");
  let log = "";
  if (publishStep) {
    try {
      log = await (await github(`/actions/jobs/${job.id}/logs`)).text();
    } catch (error) {
      process.stderr.write(`Could not load logs for ${job.name}: ${error.message}\n`);
    }
  }

  const evidenceIds = new Set([
    ...report.describes.map((describe) => describe.evidence.invocation_id),
    ...report.findings.flatMap((finding) => finding.evidence.map((evidence) => evidence.invocation_id)),
  ]);
  for (const invocationId of evidenceIds) {
    const fragment = publishStep
      ? `#step:${publishStep.number}:${evidenceLine(log, invocationId)}`
      : "";
    links.evidence.push({
      attempt_id: report.id,
      invocation_id: invocationId,
      url: `${job.html_url}${fragment}`,
    });
  }
}

await writeFile(outputPath, `${JSON.stringify(links, null, 2)}\n`);
