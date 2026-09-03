import { readStandardPackageAllowlist } from '@/lib/generated-content/allowlist';
import {
  hashCliSource,
  verifyRawCliHelp,
} from '@/lib/generated-content/cli-source';
import { PAGE_SCHEMA_VERSION } from '@/lib/generated-content/constants';
import type { GeneratedContentReader } from '@/lib/generated-content/database';

export interface ReleaseVerificationSummary {
  cli_commands: number;
  cli_payload_sha256: string;
  cli_source_sha256: string;
  package_exports: number;
  page_schema_version: number;
  reference_pages: number;
  version: string;
}

interface CliCommandTree {
  subcommands: CliCommandTree[];
}

function countCliCommands(command: CliCommandTree): number {
  return (
    1 +
    command.subcommands.reduce(
      (total, child) => total + countCliCommands(child),
      0,
    )
  );
}

export async function verifyGeneratedRelease(
  reader: GeneratedContentReader,
  version: string,
): Promise<ReleaseVerificationSummary> {
  const release = (await reader.listReleases()).find(
    (candidate) => candidate.version === version,
  );
  if (!release) {
    throw new Error(`Generated-content release ${version} does not exist.`);
  }

  const [expectedPackages, packageExports, pages, cliArtifact] =
    await Promise.all([
      readStandardPackageAllowlist(),
      reader.listPackageExports(version),
      reader.listReferencePages(version),
      reader.getCliArtifact(version),
    ]);

  const actualPackages = packageExports.map((item) => item.package_name).sort();
  const sortedExpectedPackages = [...expectedPackages].sort();
  if (
    JSON.stringify(actualPackages) !== JSON.stringify(sortedExpectedPackages)
  ) {
    throw new Error(
      `Release ${version} package set does not match the checked-in publication allowlist.`,
    );
  }

  const pageExportIds = new Set(
    pages.map((page) => String(page.package_export_id)),
  );
  for (const packageExport of packageExports) {
    if (!pageExportIds.has(String(packageExport.id))) {
      throw new Error(
        `Package ${packageExport.package_name} has no page projections.`,
      );
    }
  }

  if (!cliArtifact) {
    throw new Error(`Release ${version} has no CLI artifact.`);
  }
  verifyRawCliHelp(cliArtifact.payload.raw_help);
  const sourceHash = hashCliSource(cliArtifact.payload.raw_help);
  if (sourceHash !== cliArtifact.row.source_sha256) {
    throw new Error(`CLI source hash mismatch for release ${version}.`);
  }

  return {
    cli_commands: countCliCommands(cliArtifact.payload.root),
    cli_payload_sha256: cliArtifact.row.payload_sha256,
    cli_source_sha256: cliArtifact.row.source_sha256,
    package_exports: packageExports.length,
    page_schema_version: PAGE_SCHEMA_VERSION,
    reference_pages: pages.length,
    version,
  };
}
