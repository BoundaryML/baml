export interface TimingResult {
  med: number;
  sd: number;
  times: number[];
}

export interface WorkloadEntry {
  name: string;
  category: string;
  source?: { baml?: string; python?: string; js?: string };
  results: {
    baml?: TimingResult | null;
    python?: TimingResult | null;
    node?: TimingResult | null;
    bun?: TimingResult | null;
  };
}

export interface GitInfo {
  commit: string;
  commit_full: string;
  branch: string;
  message: string;
  commit_date: string;
  author: string;
  in_repo: boolean;
}

export interface CliInfo {
  path: string | null;
  version: string | null;
  built_at: string | null;
  git: GitInfo | null;
}

export interface RunData {
  run_id: string;
  tag?: string | null;
  timestamp: string;
  filter?: string[] | null;
  runs_per_workload: number | string;
  runners: string[];
  cli: CliInfo;
  workloads: WorkloadEntry[];
}

export interface RefEntry {
  ref: string;       // e.g. "canary/latest"
  branch: string;    // e.g. "canary"
  tag: string;       // e.g. "latest", "fast-concat"
  run_id: string;    // which run this points to
}

export interface ApiResponse {
  runs: RunData[];
  refs: RefEntry[];
}
