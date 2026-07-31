// Agent-shaped workload: spawned logical threads, depth, awaits, sysops and
// caught failures. CI invokes AgentLike(86, 14).
function AgentFrame(worker: int, depth: int) -> int {
  if (depth == 0) {
    baml.sys.sleep(baml.time.Duration.from_nanoseconds(1n));
    return worker;
  }
  AgentFrame(worker, depth - 1)
}

function AgentMaybeFail(worker: int) -> int {
  if (worker % 17 == 0) {
    throw "synthetic caught worker error"
  }
  worker
}

function AgentWorker(worker: int, depth: int) -> int throws never {
  let frame = AgentFrame(worker, depth) catch (error) {
    _ => worker
  };
  let sysop = AgentMaybeFail(worker) catch (error) {
    string => worker
  };
  frame + sysop
}

function AgentLike(workers: int, depth: int) -> int throws never {
  let tasks: baml.future.Future<int, never>[] = [];
  let worker = 0;
  while (worker < workers) {
    tasks.push(spawn { AgentWorker(worker, depth) });
    worker += 1;
  }
  let results = await baml.future.all_complete(tasks);
  results.sum()
}
