//! The stuck-task report: every unfinished task, where it is, what it waits
//! for, and each cycle of tasks waiting on each other.
//!
//! Built on demand ([`crate::BexEngine::task_report`]): the engine stops every
//! task the way a collection does, reads each one's stack and the reason it
//! parked, and asks the `baml.sync` key table who holds what. Nothing is
//! recorded while tasks run; a task only notes why it parked, which costs one
//! store.

use std::{
    collections::{HashMap, HashSet},
    fmt,
    num::NonZeroU64,
    sync::Arc,
};

use bex_external_types::BexExternalValue;
use bex_vm::StackFrame;
use bex_vm_types::{SysOp, types::FutureId};

/// Why a parked task parked, noted on its [`crate::BexThread`].
#[derive(Clone, Debug)]
pub(crate) enum Parked {
    /// Spawned, and not started yet: waiting for its `spawn with` semaphores,
    /// if any, to admit it.
    Starting,
    /// In a sys-op, such as waiting for an `atomic` key.
    SysOp(SysOp),
    /// Awaiting one future.
    Await(FutureId),
    /// Awaiting the first of several futures to settle.
    AwaitAny(Vec<FutureId>),
}

/// A task as the report saw it.
#[derive(Clone, Debug)]
pub struct StuckTask {
    /// The task id: the one `baml.sync` keys are owned by.
    pub id: NonZeroU64,
    /// Where it was spawned (the spawn's name, or the function it was spawned
    /// in); `None` for a call from the host.
    pub origin: Option<Arc<str>>,
    pub waiting: Waiting,
    /// Its BAML stack, outermost first, standard-library and native frames
    /// left out.
    pub frames: Vec<StackFrame>,
    /// The future it settles; `None` for a call from the host.
    pub(crate) settles: Option<FutureId>,
}

/// What a task in the report is waiting for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Waiting {
    /// Not parked: it was running, and paused for the report.
    Running,
    /// Spawned, and not started yet: waiting for its `spawn with` semaphores,
    /// if any, to admit it.
    Starting,
    /// `atomic` keys, held by these tasks. Empty when the keys it needs are
    /// free but it is queued behind other waiters.
    Keys(Vec<NonZeroU64>),
    /// A slot of a semaphore, through `acquire` or `with`.
    Slot,
    /// The task that settles the future it awaits; `None` when no task does
    /// (a future from the host, or one already settling).
    Task(Option<NonZeroU64>),
    /// The first of several futures to settle, and the tasks settling them.
    AnyTask(Vec<NonZeroU64>),
    /// Any other sys-op: I/O, a sleep, a host call.
    SysOp(&'static str),
}

/// One link in a deadlock: a task, and how it waits on the next one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitsOn {
    /// For a key the next task holds.
    Key,
    /// For the next task to settle the future it awaits.
    Await,
}

/// Every unfinished task, and each deadlock among them.
#[derive(Clone, Debug, Default)]
pub struct TaskReport {
    /// By task id.
    pub tasks: Vec<StuckTask>,
    /// Each cycle once, starting from its lowest task id: `(task, how it
    /// waits on the next)`, the last link pointing back to the first task.
    pub deadlocks: Vec<Vec<(NonZeroU64, WaitsOn)>>,
}

/// A task as the walk found it, before the links between tasks are known.
pub(crate) struct SeenTask {
    pub id: NonZeroU64,
    pub settles: Option<FutureId>,
    pub parked: Option<Parked>,
    pub frames: Vec<StackFrame>,
}

impl TaskReport {
    pub(crate) fn build(
        seen: Vec<SeenTask>,
        origins: &HashMap<FutureId, Arc<str>>,
        blocked_by: &[(NonZeroU64, NonZeroU64)],
    ) -> Self {
        let settler: HashMap<FutureId, NonZeroU64> = seen
            .iter()
            .filter_map(|task| task.settles.map(|future| (future, task.id)))
            .collect();
        let mut tasks: Vec<StuckTask> = seen
            .into_iter()
            .map(|task| {
                let waiting = match task.parked {
                    None => Waiting::Running,
                    Some(Parked::Starting) => Waiting::Starting,
                    Some(Parked::SysOp(op)) => match op {
                        SysOp::BamlSyncWait => Waiting::Keys(
                            blocked_by
                                .iter()
                                .filter(|(waiter, _)| *waiter == task.id)
                                .map(|(_, holder)| *holder)
                                .collect(),
                        ),
                        SysOp::BamlSyncWaitPermit => Waiting::Slot,
                        op => Waiting::SysOp(op.path()),
                    },
                    Some(Parked::Await(future)) => Waiting::Task(settler.get(&future).copied()),
                    Some(Parked::AwaitAny(futures)) => Waiting::AnyTask(
                        futures
                            .iter()
                            .filter_map(|future| settler.get(future).copied())
                            .collect(),
                    ),
                };
                StuckTask {
                    id: task.id,
                    settles: task.settles,
                    origin: task
                        .settles
                        .and_then(|future| origins.get(&future).cloned()),
                    waiting,
                    frames: task
                        .frames
                        .into_iter()
                        .filter(|frame| !frame.is_builtin() && !frame.file_path.is_empty())
                        .collect(),
                }
            })
            .collect();
        tasks.sort_by_key(|task| task.id);
        let deadlocks = find_cycles(&tasks);
        Self { tasks, deadlocks }
    }

    /// Whether any task was found.
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// The task settling `future` and every task it waits on, directly or
    /// through others, with the deadlocks among them: what is holding that
    /// one task up. Empty when no task settles `future`.
    pub(crate) fn holding_up(&self, future: FutureId) -> Self {
        let Some(first) = self.task_settling(future) else {
            return Self::default();
        };
        let mut keep = HashSet::from([first.id]);
        let mut pending = vec![first];
        while let Some(task) = pending.pop() {
            let waits_on: &[NonZeroU64] = match &task.waiting {
                Waiting::Keys(ids) | Waiting::AnyTask(ids) => ids,
                Waiting::Task(Some(id)) => std::slice::from_ref(id),
                _ => &[],
            };
            for id in waits_on {
                if keep.insert(*id)
                    && let Some(next) = self.tasks.iter().find(|task| task.id == *id)
                {
                    pending.push(next);
                }
            }
        }
        let tasks: Vec<StuckTask> = self
            .tasks
            .iter()
            .filter(|task| keep.contains(&task.id))
            .cloned()
            .collect();
        let deadlocks = find_cycles(&tasks);
        Self { tasks, deadlocks }
    }

    /// The task settling `future`, if one is still running.
    pub(crate) fn task_settling(&self, future: FutureId) -> Option<&StuckTask> {
        self.tasks.iter().find(|task| task.settles == Some(future))
    }
}

impl StuckTask {
    /// Its stack as a `baml.errors.StackTrace`, for `Future.stack_trace`.
    pub(crate) fn stack_trace_value(&self) -> BexExternalValue {
        const FRAME: &str = "baml.errors.StackFrame";
        let frames = self
            .frames
            .iter()
            .map(|frame| BexExternalValue::Instance {
                class_name: FRAME.to_string(),
                type_args: Vec::new(),
                fields: indexmap::indexmap! {
                    "file".to_string() => BexExternalValue::String(frame.file_path.as_str().into()),
                    "line".to_string() => BexExternalValue::Int(
                        i64::try_from(frame.error_line).unwrap_or(i64::MAX),
                    ),
                    "function_name".to_string() =>
                        BexExternalValue::String(frame.function_name.as_str().into()),
                },
            })
            .collect();
        BexExternalValue::Instance {
            class_name: "baml.errors.StackTrace".to_string(),
            type_args: Vec::new(),
            fields: indexmap::indexmap! {
                "frames".to_string() => BexExternalValue::Array {
                    element_type: baml_type::RealizedTy::Class(
                        bex_external_types::WireHead::Static(
                            baml_type::TypeName::from_dotted_path(FRAME),
                        ),
                        Box::new([]),
                    ),
                    items: frames,
                },
            },
        }
    }
}

/// Each simple cycle of "waits on", once, from its lowest task id. A key wait
/// or a single await is a link: the task cannot go on until that one task
/// does. A semaphore slot or a wait for the first of several futures is not:
/// any of several tasks could free it.
fn find_cycles(tasks: &[StuckTask]) -> Vec<Vec<(NonZeroU64, WaitsOn)>> {
    let links: HashMap<NonZeroU64, Vec<(NonZeroU64, WaitsOn)>> = tasks
        .iter()
        .map(|task| {
            let next = match &task.waiting {
                Waiting::Keys(holders) => holders.iter().map(|id| (*id, WaitsOn::Key)).collect(),
                Waiting::Task(Some(id)) => vec![(*id, WaitsOn::Await)],
                _ => Vec::new(),
            };
            (task.id, next)
        })
        .collect();
    let mut cycles = Vec::new();
    for task in tasks {
        let mut path = Vec::new();
        walk(task.id, task.id, &links, &mut path, &mut cycles);
    }
    cycles
}

/// Extend `path` (which starts at `start`) from `at`, recording a cycle each
/// time a link returns to `start`. Only visits tasks above `start`, so each
/// cycle is found once, from its lowest id.
fn walk(
    start: NonZeroU64,
    at: NonZeroU64,
    links: &HashMap<NonZeroU64, Vec<(NonZeroU64, WaitsOn)>>,
    path: &mut Vec<(NonZeroU64, WaitsOn)>,
    cycles: &mut Vec<Vec<(NonZeroU64, WaitsOn)>>,
) {
    const MAX_CYCLES: usize = 16;
    for &(next, how) in links.get(&at).into_iter().flatten() {
        if cycles.len() >= MAX_CYCLES {
            return;
        }
        path.push((at, how));
        if next == start {
            cycles.push(path.clone());
        } else if next > start && !path.iter().any(|(id, _)| *id == next) {
            walk(start, next, links, path, cycles);
        }
        path.pop();
    }
}

/// Nothing for an empty report.
impl fmt::Display for TaskReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let count = self.tasks.len();
        if count == 0 {
            return Ok(());
        }
        writeln!(
            f,
            "{count} {} still running:",
            if count == 1 { "task" } else { "tasks" }
        )?;
        for task in &self.tasks {
            write!(f, "{task}")?;
        }
        for cycle in &self.deadlocks {
            write!(f, "deadlock: task {}", cycle[0].0)?;
            for (i, (_, how)) in cycle.iter().enumerate() {
                let next = cycle.get(i + 1).map_or(cycle[0].0, |(id, _)| *id);
                let link = match how {
                    WaitsOn::Key => "waits for a key held by",
                    WaitsOn::Await => "awaits",
                };
                if i == 0 {
                    write!(f, " {link} task {next}")?;
                } else {
                    write!(f, ", which {link} task {next}")?;
                }
            }
            writeln!(f)?;
        }
        Ok(())
    }
}

/// `task N (spawned in f), what it waits for`, then its stack, a line per
/// frame.
impl fmt::Display for StuckTask {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "task {}", self.id)?;
        match &self.origin {
            Some(origin) => write!(f, " (spawned in {origin})")?,
            None => write!(f, " (called from the host)")?,
        }
        writeln!(f, ", {}", Describe(&self.waiting))?;
        for frame in &self.frames {
            writeln!(
                f,
                "  File \"{}\", line {}, in {}",
                frame.file_path, frame.error_line, frame.function_name
            )?;
        }
        Ok(())
    }
}

struct Describe<'a>(&'a Waiting);

impl fmt::Display for Describe<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let tasks = |ids: &[NonZeroU64]| {
            ids.iter()
                .map(|id| format!("task {id}"))
                .collect::<Vec<_>>()
                .join(", ")
        };
        match self.0 {
            Waiting::Running => write!(f, "running"),
            Waiting::Starting => write!(f, "not started yet"),
            Waiting::Keys(holders) if holders.is_empty() => {
                write!(f, "waiting behind other tasks queued for its keys")
            }
            Waiting::Keys(holders) => write!(f, "waiting for a key held by {}", tasks(holders)),
            Waiting::Slot => write!(f, "waiting for a semaphore slot"),
            Waiting::Task(Some(id)) => write!(f, "awaiting task {id}"),
            Waiting::Task(None) => write!(f, "awaiting a future"),
            Waiting::AnyTask(ids) if ids.is_empty() => write!(f, "awaiting futures"),
            Waiting::AnyTask(ids) => write!(f, "awaiting the first of {}", tasks(ids)),
            Waiting::SysOp(path) => write!(f, "in {path}"),
        }
    }
}
