'use client';

type DocsVersion = {
  version: string;
  channel: string;
};

export function VersionSwitcher({
  currentVersion,
  routes,
  versions,
}: {
  currentVersion: string;
  routes: Record<string, string>;
  versions: DocsVersion[];
}) {
  return (
    <label className="shadcn-version-switcher">
      <span>Version</span>
      <select
        aria-label="BAML documentation version"
        value={currentVersion}
        onChange={(event) => {
          window.location.assign(routes[event.target.value] ?? routes[currentVersion]);
        }}
      >
        {versions.map((entry) => (
          <option key={entry.version} value={entry.version}>
            v{entry.version} ({entry.channel})
          </option>
        ))}
      </select>
    </label>
  );
}
