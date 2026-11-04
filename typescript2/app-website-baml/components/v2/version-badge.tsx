const MANIFEST = 'https://pkg.boundaryml.com/manifest/v1/canary.json';
const RELEASE =
  'https://github.com/boundaryml/baml/releases/tag/baml-language-';

/** The version `baml` installs by default, from the manifest the installer reads. Checked hourly. */
async function canaryVersion(): Promise<string | null> {
  try {
    const res = await fetch(MANIFEST, { next: { revalidate: 3600 } });
    if (!res.ok) return null;
    const { version } = (await res.json()) as { version?: unknown };
    return typeof version === 'string' && /^[\w.-]+$/.test(version)
      ? version
      : null;
  } catch {
    return null;
  }
}

/** Renders nothing if the manifest can't be read, so the hero never shows a stale or broken version. */
export default async function VersionBadge({ left }: { left?: boolean }) {
  const version = await canaryVersion();
  if (!version) return null;
  return (
    <div className={`flex mb-6 md:mb-8 ${left ? '' : 'justify-center'}`}>
      <a
        className="focus-ring inline-flex items-center gap-2 h-7 px-3 rounded-full border border-white/15 bg-black text-[13px] text-white/70 hover:border-white/40 hover:text-white transition-colors"
        href={`${RELEASE}${version}`}
      >
        <span className="size-1.5 rounded-full bg-clover" />
        Beta v{version}
      </a>
    </div>
  );
}
