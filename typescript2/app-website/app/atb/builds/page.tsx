"use client";

import { motion } from "framer-motion";
import { useAtbState, bamlRefLabel } from "@/app/atb/_lib/api";

import { bytes, shortSha, timeAgo } from "@/app/atb/_lib/format";
import { EASE, EmptyState, Skeleton, StatusPill } from "@/app/atb/_components/ui";
import { useNow } from "@/app/atb/_components/use-now";

export default function BuildsPage() {
  const now = useNow();
  const state = useAtbState();
  const builds = state?.builds;
  const trophies = state?.trophies;

  const runsForSha = (sha: string) =>
    (trophies ?? []).filter((t) => t.bamlVersion === sha && !t.isCohortReport);

  return (
    <div className="pt-12">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: EASE }}
      >
        <h1 className="font-atb-serif text-3xl font-semibold tracking-tight">
          Builds
        </h1>
        <p className="text-atb-ink-3 mt-1 text-sm">
          Nightly baml CLI builds. Each run pins one of these shas.
        </p>
      </motion.div>

      {!builds ? (
        <div className="space-y-2 mt-8">
          {[...Array(4)].map((_, i) => (
            <Skeleton key={i} className="h-14" />
          ))}
        </div>
      ) : builds.length === 0 ? (
        <EmptyState label="no builds yet" />
      ) : (
        <div className="border border-atb-line rounded-2xl overflow-hidden bg-atb-ivory/40 mt-8">
          <table className="w-full text-sm">
            <thead>
              <tr className="text-[11px] uppercase tracking-wider text-atb-ink-3 border-b border-atb-line">
                <th className="text-left font-medium px-4 py-2.5">Version</th>
                <th className="text-left font-medium px-3 py-2.5 hidden sm:table-cell">
                  Sha
                </th>
                <th className="text-left font-medium px-3 py-2.5 w-24">
                  Status
                </th>
                <th className="text-right font-medium px-3 py-2.5 w-24 hidden md:table-cell">
                  Size
                </th>
                <th className="text-right font-medium px-3 py-2.5 w-16">
                  Runs
                </th>
                <th className="text-right font-medium px-4 py-2.5 w-24">
                  Built
                </th>
              </tr>
            </thead>
            <tbody>
              {builds.map((b, i) => (
                <motion.tr
                  key={b._id}
                  initial={{ opacity: 0, y: 8 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{ duration: 0.45, delay: Math.min(i * 0.05, 0.4), ease: EASE }}
                  className="border-b border-atb-line/60 last:border-0 hover:bg-atb-oat/40 transition-colors"
                >
                  <td className="px-4 py-3 font-atb-mono text-atb-ink">
                    {bamlRefLabel(b.ref)}
                  </td>
                  <td className="px-3 py-3 font-atb-mono text-atb-ink-3 hidden sm:table-cell">
                    {shortSha(b.sha)}
                  </td>
                  <td className="px-3 py-3">
                    <StatusPill status={b.status} />
                  </td>
                  <td className="px-3 py-3 text-right font-atb-mono text-atb-ink-2 hidden md:table-cell">
                    {bytes(b.sizeBytes)}
                  </td>
                  <td className="px-3 py-3 text-right font-atb-mono text-atb-ink-2">
                    {runsForSha(b.sha).length}
                  </td>
                  <td className="px-4 py-3 text-right text-atb-ink-3 text-xs whitespace-nowrap">
                    {b.builtAt ? timeAgo(b.builtAt, now) : "—"}
                  </td>
                </motion.tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
