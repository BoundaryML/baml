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
 * Labels for a whole list, disambiguated so no two tabs read the same.
 *
 * Two checkouts of one project, or two projects whose directories happen to
 * agree, would otherwise be indistinguishable; a repeated label keeps its
 * parent directory as a prefix. The full path is still the tooltip.
 */
export function projectLabels(projects: ProjectEntry[]): Map<string, string> {
  const labels = new Map<string, string>();
  const counts = new Map<string, number>();
  for (const project of projects) {
    const label = projectLabel(project.path, project.name);
    counts.set(label, (counts.get(label) ?? 0) + 1);
  }
  for (const project of projects) {
    const label = projectLabel(project.path, project.name);
    if ((counts.get(label) ?? 0) < 2) {
      labels.set(project.path, label);
      continue;
    }
    const trimmed = project.path.replace(/[/\\]+$/, '');
    const parent = basename(
      trimmed.slice(0, trimmed.length - basename(trimmed).length),
    );
    labels.set(project.path, parent ? `${parent}/${label}` : label);
  }
  return labels;
}
