import { parseToml, type TomlTable, type TomlValue } from "./toml.ts";

/** A crate downloaded from a cargo registry (crates.io and friends). */
export interface RegistryPackage {
  name: string;
  version: string;
  /** sha256 of the `.crate` file, as recorded in Cargo.lock. */
  checksum: string;
  /** The full `registry+...` source string. */
  source: string;
  /** `<name>-<version>.crate` */
  crateFile: string;
}

/** A git dependency. */
export interface GitPackage {
  name: string;
  version: string;
  /** Canonical git URL with the `git+` prefix and `#rev` fragment removed. */
  url: string;
  /** The pinned revision (commit / tag), if present in the source fragment. */
  rev: string | null;
}

export interface CargoLock {
  version: number | null;
  registry: RegistryPackage[];
  git: GitPackage[];
}

const CRATES_IO_REGISTRY = "registry+https://github.com/rust-lang/crates.io-index";

function asString(v: TomlValue | undefined): string | undefined {
  return typeof v === "string" ? v : undefined;
}

/**
 * Reduce a git source URL to the form cargo canonicalises on, for keying:
 * strip the `git+` scheme prefix and the `#<rev>` fragment. We keep any
 * `?branch=`/`?tag=`/`?rev=` query since cargo treats those as distinct sources.
 */
export function canonicalGitUrl(source: string): { url: string; rev: string | null } {
  let s = source;
  if (s.startsWith("git+")) s = s.slice(4);
  let rev: string | null = null;
  const hash = s.indexOf("#");
  if (hash >= 0) {
    rev = s.slice(hash + 1) || null;
    s = s.slice(0, hash);
  }
  return { url: s, rev };
}

export function parseCargoLock(text: string): CargoLock {
  const doc: TomlTable = parseToml(text);

  const version = typeof doc.version === "number" ? doc.version : null;

  const registry: RegistryPackage[] = [];
  const git: GitPackage[] = [];

  const packages = doc.package;
  if (Array.isArray(packages)) {
    for (const entry of packages) {
      if (typeof entry !== "object" || Array.isArray(entry)) continue;
      const pkg = entry as TomlTable;
      const name = asString(pkg.name);
      const ver = asString(pkg.version);
      const source = asString(pkg.source);
      if (!name || !ver || !source) {
        // Path dependencies (workspace members) have no source — nothing to cache.
        continue;
      }

      if (source.startsWith("registry+")) {
        const checksum = asString(pkg.checksum);
        if (!checksum) continue; // No checksum → can't content-address; skip.
        registry.push({
          name,
          version: ver,
          checksum,
          source,
          crateFile: `${name}-${ver}.crate`,
        });
      } else if (source.startsWith("git+")) {
        const { url, rev } = canonicalGitUrl(source);
        git.push({ name, version: ver, url, rev });
      }
    }
  }

  // Some older lockfiles record checksums in a [metadata] table instead of inline.
  const metadata = doc.metadata;
  if (typeof metadata === "object" && !Array.isArray(metadata)) {
    for (const [key, value] of Object.entries(metadata as TomlTable)) {
      // Keys look like: "checksum <name> <version> (registry+https://...)"
      const m = /^checksum\s+(\S+)\s+(\S+)\s+\((registry\+[^)]+)\)$/.exec(key);
      if (!m) continue;
      const sum = asString(value);
      if (!sum || sum === "<none>") continue;
      const [, name, ver] = m;
      // Only add if we didn't already capture it inline.
      const already = registry.some((p) => p.name === name && p.version === ver);
      if (!already) {
        registry.push({
          name: name!,
          version: ver!,
          checksum: sum,
          source: m[3]!,
          crateFile: `${name}-${ver}.crate`,
        });
      }
    }
  }

  return { version, registry, git };
}

export function isCratesIo(source: string): boolean {
  return source === CRATES_IO_REGISTRY;
}
