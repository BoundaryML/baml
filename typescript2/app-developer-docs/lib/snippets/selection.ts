import type { ProjectSnippet, ProjectSnippetFile } from './discovery';

/** Display excerpts from the same complete project that the compiler checks. */
export function selectProjectFiles(
  project: ProjectSnippet,
  file?: string,
  regions?: string[],
): ProjectSnippetFile[] {
  if (!file) {
    if (regions) throw new Error('Project regions require a file');
    return project.files;
  }
  const selected = project.files.find((entry) => entry.projectPath === file);
  if (!selected) throw new Error(`Project ${project.id} has no file ${file}`);
  if (!regions) return [selected];
  if (regions.length === 0)
    throw new Error('Select at least one project region');
  const displaySource = regions
    .map((region) => {
      const code = selected.regions?.get(region);
      if (code === undefined) {
        throw new Error(
          `Project ${project.id}/${file} has no region ${region}`,
        );
      }
      return code;
    })
    .join('\n\n');
  return [{ ...selected, displaySource }];
}
