import type {
  ProjectCatalogEntry,
  ProjectRuntimeStatus,
  ProjectUpdate,
} from './worker-protocol';

export interface ProjectIdentity {
  project: string;
  incarnation?: number;
  sourceRevision?: number;
}

export interface CatalogChange {
  entries: ProjectIdentity[];
  /** Removed projects and remove/re-add identity changes. */
  purgedProjects: Set<string>;
  /** Same-incarnation source advances that invalidate an older snapshot. */
  advancedProjects: Set<string>;
}

/**
 * Build one sorted, duplicate-free catalog. `projects` remains the path source
 * of truth for compatibility; metadata is applied only to matching entries.
 */
export function normalizeProjectCatalog(
  projects: readonly string[],
  entries?: readonly ProjectCatalogEntry[],
): ProjectIdentity[] {
  const metadata = new Map(entries?.map((entry) => [entry.project, entry]));
  return [...new Set(projects)]
    .sort((a, b) => a.localeCompare(b))
    .map((project) => {
      const entry = metadata.get(project);
      return entry
        ? {
            project,
            incarnation: entry.incarnation,
            sourceRevision: entry.sourceRevision,
          }
        : { project };
    });
}

export function projectIdentityKey(identity: ProjectIdentity | undefined): string {
  if (!identity) return '';
  return `${identity.project}\u0000${identity.incarnation ?? 'legacy'}`;
}

/**
 * Tracks the frontend's monotonic identity/revision watermark. It deliberately
 * owns no React state or payloads, which keeps remove/re-add cache eviction in
 * the caller explicit and testable.
 */
export class ProjectPayloadFencer {
  private catalog = new Map<string, ProjectIdentity>();
  private sourceWatermarks = new Map<string, number>();
  private initialized = false;

  constructor(private readonly sessionEpoch?: number) {}

  /** Legacy payloads are accepted only by a legacy, unbound fencer. */
  acceptSession(payloadSessionEpoch?: number): boolean {
    return this.sessionEpoch === undefined
      ? payloadSessionEpoch === undefined
      : payloadSessionEpoch === this.sessionEpoch;
  }

  applyCatalog(
    projects: readonly string[],
    entries?: readonly ProjectCatalogEntry[],
  ): CatalogChange {
    const normalized = normalizeProjectCatalog(projects, entries).map((entry) => {
      const previous = this.catalog.get(entry.project);
      if (!previous) return entry;
      if (
        previous.incarnation !== undefined &&
        entry.incarnation === undefined
      ) {
        return previous;
      }
      if (
        previous.incarnation !== undefined &&
        entry.incarnation !== undefined &&
        entry.incarnation < previous.incarnation
      ) {
        return previous;
      }
      if (
        previous.incarnation === entry.incarnation &&
        previous.sourceRevision !== undefined &&
        entry.sourceRevision !== undefined &&
        entry.sourceRevision < previous.sourceRevision
      ) {
        return previous;
      }
      return entry;
    });
    const next = new Map(normalized.map((entry) => [entry.project, entry]));
    const purgedProjects = new Set<string>();
    const advancedProjects = new Set<string>();

    for (const [project, previous] of this.catalog) {
      const current = next.get(project);
      if (!current || incarnationChanged(previous, current)) {
        purgedProjects.add(project);
        this.sourceWatermarks.delete(project);
      }
    }

    for (const entry of normalized) {
      const previous = this.catalog.get(entry.project);
      if (
        previous &&
        !incarnationChanged(previous, entry) &&
        entry.sourceRevision !== undefined &&
        (previous.sourceRevision === undefined ||
          entry.sourceRevision > previous.sourceRevision)
      ) {
        advancedProjects.add(entry.project);
      }

      if (entry.sourceRevision !== undefined) {
        const watermark = this.sourceWatermarks.get(entry.project);
        this.sourceWatermarks.set(
          entry.project,
          watermark === undefined
            ? entry.sourceRevision
            : Math.max(watermark, entry.sourceRevision),
        );
      }
    }

    this.catalog = next;
    this.initialized = true;
    return { entries: normalized, purgedProjects, advancedProjects };
  }

  identity(project: string): ProjectIdentity | undefined {
    return this.catalog.get(project);
  }

  /**
   * Reject late payloads for removed/re-added projects and regressive source
   * revisions. Once a qualified catalog is seen, missing qualification is also
   * rejected; legacy catalogs continue accepting legacy payloads.
   */
  accept(
    project: string,
    projectIncarnation?: number,
    sourceRevision?: number,
  ): boolean {
    const identity = this.catalog.get(project);
    if (this.initialized && !identity) return false;

    if (identity?.incarnation !== undefined) {
      if (projectIncarnation !== identity.incarnation) return false;
    } else if (projectIncarnation !== undefined && identity) {
      // A qualified payload cannot be matched safely to a legacy catalog row.
      return false;
    }

    const watermark = this.sourceWatermarks.get(project);
    if (watermark !== undefined) {
      if (sourceRevision === undefined || sourceRevision < watermark) return false;
    }

    if (sourceRevision !== undefined) {
      this.sourceWatermarks.set(
        project,
        watermark === undefined ? sourceRevision : Math.max(watermark, sourceRevision),
      );
    }
    return true;
  }
}

export function runtimeStatusFromUpdate(update: ProjectUpdate): ProjectRuntimeStatus {
  if (update.runtime) return update.runtime;

  const hasErrors = update.diagnostics.some((diag) => diag.severity === 'error');
  const revision = update.sourceRevision ?? 0;
  return {
    state: update.isBexCurrent
      ? 'ready'
      : hasErrors
        ? 'blockedByDiagnostics'
        : 'idleStale',
    requestedRevision: revision,
    installedRevision: update.isBexCurrent ? revision : null,
    hasLastKnownGood: update.functions.length > 0,
  };
}

export function preparingRuntimeStatus(
  identity: ProjectIdentity,
  previous?: ProjectRuntimeStatus,
): ProjectRuntimeStatus {
  return {
    state: 'building',
    requestedRevision: Math.max(
      identity.sourceRevision ?? 0,
      previous?.requestedRevision ?? 0,
    ),
    installedRevision: previous?.installedRevision ?? null,
    generation: previous?.generation ?? null,
    hasLastKnownGood: previous?.hasLastKnownGood ?? false,
  };
}

export function runtimeIsReady(status: ProjectRuntimeStatus | undefined): boolean {
  if (!status || status.state !== 'ready') return false;
  return (
    status.installedRevision === undefined ||
    status.installedRevision === status.requestedRevision
  );
}

/** Accept equal epochs (incremental expansion in one collection), reject ABA regressions. */
export function acceptMonotonicEpoch(
  watermarks: Map<string, number>,
  key: string,
  epoch: number,
): boolean {
  const previous = watermarks.get(key);
  if (previous !== undefined && epoch < previous) return false;
  watermarks.set(key, epoch);
  return true;
}

function incarnationChanged(a: ProjectIdentity, b: ProjectIdentity): boolean {
  return (
    a.incarnation !== undefined &&
    b.incarnation !== undefined &&
    a.incarnation !== b.incarnation
  );
}
