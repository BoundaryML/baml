/**
 * How a project is shown to a person.
 *
 * A project's identity is its root path, which is what every playground
 * message names it by, but a path is not a label: absolute paths are long,
 * they share prefixes, and a column of them tells you nothing at a glance.
 *
 * A package that names itself in `[package].name` is shown by that name.
 * Most do not, and the honest fallback is the directory the project lives in,
 * NOT the compiler's unnamed default: that default is the same string for
 * every unnamed package, so it would label every tab identically.
 */

/** A project as the server lists it: identity, and a name if it declares one. */
export interface ProjectEntry {
  path: string;
  name?: string;
}

/** The directory component of a path, with either separator. */
function basename(path: string): string {
  const trimmed = path.replace(/[/\\]+$/, '');
  const cut = Math.max(trimmed.lastIndexOf('/'), trimmed.lastIndexOf('\\'));
  return cut === -1 ? trimmed : trimmed.slice(cut + 1);
}

/**
 * The short label for a project root.
 *
 * Falls back to the project's directory, and past a directory literally
 * called `baml_src` to the one holding it, since a column of tabs all reading
 * "baml_src" is no better than a column of paths.
 */
export function projectLabel(path: string, name?: string): string {
  if (name) {
    return name;
  }
  const directory = basename(path);
  if (directory !== 'baml_src') {
    return directory || path;
  }
  const parent = basename(
    path.replace(/[/\\]+$/, '').slice(0, -directory.length),
  );
  return parent || directory;
}

/**
 * One project as any server version sends it.
 *
 * A server older than this client lists projects as bare paths, which is a
 * live case rather than a hypothetical: the toolchain wrapper exists to pin an
 * older toolchain per project, so a newer client talking to an older server is
 * the designed arrangement.
 */
export function toProjectEntry(project: ProjectEntry | string): ProjectEntry {
  return typeof project === 'string' ? { path: project } : project;
}

/** A path's non-empty components, with either separator. */
function segments(path: string): string[] {
  return path
    .replace(/[/\\]+$/, '')
    .split(/[/\\]+/)
    .filter(Boolean);
}

/**
 * Progressively more specific labels for one project: the short label, then
 * the same with one more ancestor directory prefixed, and so on up the path.
 */
function labelCandidates(project: ProjectEntry): string[] {
  const parts = segments(project.path);
  const base = projectLabel(project.path, project.name);
  // Trailing components the base already speaks for, so an ancestor is not
  // repeated: the directory, or the directory and a `baml_src` below it.
  const consumed = !project.name && parts.at(-1) === 'baml_src' ? 2 : 1;
  const ancestors = parts.slice(0, Math.max(0, parts.length - consumed));
  const candidates = [base];
  for (let depth = 1; depth <= ancestors.length; depth++) {
    candidates.push(
      [...ancestors.slice(ancestors.length - depth), base].join('/'),
    );
  }
  return candidates;
}

/**
 * Labels for a whole list, disambiguated so no two tabs read the same.
 *
 * Projects whose short labels collide keep taking one more ancestor directory
 * until the whole group reads differently. One ancestor is not enough on its
 * own: two checkouts of the same repository agree for as many components as
 * they share, and stopping at the first would leave both tabs identical. When
 * a group's paths are exhausted the full path is the label, since nothing
 * shorter can tell them apart.
 */
export function projectLabels(projects: ProjectEntry[]): Map<string, string> {
  const groups = new Map<string, ProjectEntry[]>();
  for (const project of projects) {
    const label = projectLabel(project.path, project.name);
    const group = groups.get(label);
    if (group) {
      group.push(project);
    } else {
      groups.set(label, [project]);
    }
  }

  const labels = new Map<string, string>();
  for (const group of groups.values()) {
    const first = group[0];
    if (group.length === 1 && first) {
      labels.set(first.path, projectLabel(first.path, first.name));
      continue;
    }
    const candidates = group.map(labelCandidates);
    const deepest = Math.max(...candidates.map((one) => one.length - 1));
    let depth = 0;
    for (let next = 1; next <= deepest; next++) {
      const distinct = new Set(
        candidates.map((one) => one[Math.min(next, one.length - 1)]),
      );
      depth = next;
      if (distinct.size === candidates.length) {
        break;
      }
    }
    const distinct = new Set(
      candidates.map((one) => one[Math.min(depth, one.length - 1)]),
    );
    group.forEach((project, index) => {
      const own = candidates[index];
      labels.set(
        project.path,
        distinct.size === candidates.length && own
          ? (own[Math.min(depth, own.length - 1)] ?? project.path)
          : project.path,
      );
    });
  }
  return labels;
}
