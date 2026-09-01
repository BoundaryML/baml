import { spawnSync } from "node:child_process"
import { readdirSync, readFileSync, existsSync } from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const repoRoot = path.resolve(appRoot, "../../..")
const examplesRoot = path.join(appRoot, "examples")
const cargoManifest = path.join(repoRoot, "baml_language/Cargo.toml")
const binary = path.join(repoRoot, "baml_language/target/debug/baml-cli")

function manifests(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const absolute = path.join(directory, entry.name)
    if (entry.isDirectory()) return manifests(absolute)
    return entry.name === "example.json" ? [absolute] : []
  })
}

function run(command, args, cwd = repoRoot) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8", stdio: "pipe" })
  if (result.stdout) process.stdout.write(result.stdout)
  if (result.stderr) process.stderr.write(result.stderr)
  if (result.status !== 0) process.exit(result.status ?? 1)
}

if (!existsSync(binary)) {
  run("cargo", ["build", "--manifest-path", cargoManifest, "-p", "baml_cli"])
}

const fixtures = manifests(examplesRoot)
if (fixtures.length === 0) throw new Error("No documentation example fixtures were found")

for (const manifestPath of fixtures) {
  const manifest = JSON.parse(readFileSync(manifestPath, "utf8"))
  if (!["check", "run"].includes(manifest.mode)) {
    throw new Error(`Native validation for mode '${manifest.mode}' is not implemented (${manifest.id})`)
  }
  const fixtureRoot = path.dirname(manifestPath)
  process.stdout.write(`\nValidating ${manifest.id} (${manifest.mode})\n`)
  run(binary, ["check", "--project", fixtureRoot])
}

process.stdout.write(`\nValidated ${fixtures.length} documentation fixture(s) with the native compiler.\n`)
