//! The definitions each file carries (format minor 17): every function, call
//! path, thread and clock epoch its own records reference, so a reader turns
//! the file into rows alone.
//!
//! A definition arrives once, when the runtime first defines it. The recorder
//! keeps what a later file may need again: the clock epochs (a few per
//! process), and each thread with the call paths it defined (every thread
//! defines its own) while it may still be referenced. A finished thread is
//! forgotten [`GRACE`] after the file holding its completion seals, once no
//! thread spawned at one of its paths is kept (their paths lead up through
//! it) and no request it made is open. The grace covers the aggregates the
//! processor flushes after a thread's last call.
//!
//! What the open file references is a set per kind, cleared at seal. A
//! reference costs a comparison with the last one of its kind, else one set
//! lookup; only the first in a file writes the definition, with what it
//! references in turn: a path's parents up to the top, its functions and its
//! thread; a thread's epoch, with its latest observed state, and its spawn
//! path. A function's full definition is
//! written once per recording, in the first file that references it; later
//! files carry its slim one (`btel_core::recording::slim_function`).

use std::{collections::VecDeque, time::Duration};

use btel_core::recording::field;
use btel_types::FunctionId;
use rustc_hash::{FxHashMap, FxHashSet};
use web_time::Instant;

use crate::{functions::FunctionDefinitions, proto};

/// Where the open file's definitions go, and their encoded size.
pub(crate) struct Out<'a> {
    pub(crate) definitions: &'a mut proto::Definitions,
    pub(crate) clock_states: &'a mut Vec<proto::ClockEpochState>,
    pub(crate) bytes: &'a mut usize,
    pub(crate) functions: &'a mut FunctionDefinitions,
}

fn push<M: prost::Message>(output: &mut Vec<M>, bytes: &mut usize, tag: u32, value: M) {
    *bytes = bytes.saturating_add(prost::encoding::message::encoded_len(tag, &value));
    output.push(value);
}

/// How many seals a record waits for definitions not known yet. A finished
/// thread, and a placeholder for one never defined, is kept at least one seal
/// longer, so a held record finds its definitions.
pub(crate) const HOLD_SEALS: u8 = 4;

/// How many threads' paths are kept after their placeholders went, for a
/// definition that arrives late.
const EXPIRED_THREADS: usize = 4096;

/// How long a finished thread's definitions are kept after the file with its
/// completion seals: the processor flushes the aggregates of its calls within
/// one cache-flush interval.
pub(crate) const GRACE: Duration =
    btel_settings::processor::CACHE_FLUSH_INTERVAL_DURATION.saturating_mul(2);

/// A thread, or a placeholder for the paths of one not defined yet.
#[derive(Default)]
struct Thread {
    definition: Option<proto::ThreadDefinition>,
    /// The call paths it defined.
    paths: Vec<u32>,
    /// The thread that owns its spawn path, which it keeps.
    spawner: Option<u64>,
    /// Kept threads spawned at one of its paths.
    spawned: u32,
    /// Its completion was written.
    finished: bool,
    /// Its grace ran out: it goes once nothing keeps it.
    retired: bool,
}

/// A call path as defined, with the functions it names.
struct Path {
    definition: proto::CallPathDefinition,
    callee: FunctionId,
    visible_caller: Option<FunctionId>,
}

/// What the open file references.
#[derive(Default)]
struct InFile {
    functions: FxHashSet<FunctionId>,
    paths: FxHashSet<u32>,
    threads: FxHashSet<u64>,
    epochs: FxHashSet<u64>,
    /// The last path and thread referenced, in this file; 0 for none.
    last_path: u32,
    last_thread: u64,
    /// Referenced before their definitions arrived; tried again at seal.
    missing_paths: FxHashSet<u32>,
    missing_threads: FxHashSet<u64>,
}

#[derive(Default)]
pub(crate) struct FileDefinitions {
    paths: FxHashMap<u32, Path>,
    threads: FxHashMap<u64, Thread>,
    epochs: FxHashMap<u64, proto::ClockEpochDefinition>,
    /// The latest observed state of each epoch.
    states: FxHashMap<u64, proto::ClockEpochState>,
    /// Threads whose completion is in the open file.
    finished: Vec<u64>,
    /// Finished threads waiting out their grace, oldest first, with the
    /// time and the seal they finished at.
    retiring: VecDeque<(Instant, u64, u64)>,
    /// Placeholders made for the paths of a thread not defined, with the
    /// seal they were made at: one never defined goes like a finished one.
    provisional: VecDeque<(u64, u64)>,
    /// Threads defined before their spawn path, by that path: linked to the
    /// path's thread, which they keep, once it is defined.
    unlinked: FxHashMap<u32, Vec<u64>>,
    /// The paths of placeholders that went with their thread still not
    /// defined, kept for that thread's arrival, oldest first, at most
    /// [`EXPIRED_THREADS`] threads' worth.
    expired: FxHashMap<u64, Vec<(u32, Path)>>,
    expired_order: VecDeque<u64>,
    /// Seals so far.
    seals: u64,
    /// [`GRACE`], or what a test sets.
    pub(crate) grace: Option<Duration>,
    /// Seals a finished thread waits past its file's: [`HOLD_SEALS`] + 1, or
    /// what a test sets.
    pub(crate) grace_seals: Option<u64>,
    /// Paths whose chain is defined up to the top, with every thread on it
    /// ready; and the last path and thread found ready.
    ready_paths: FxHashSet<u32>,
    last_ready_path: u32,
    last_ready_thread: u64,
    /// The threads whose readiness is being checked, innermost last.
    checking: Vec<u64>,
    file: InFile,
}

impl FileDefinitions {
    /// Call path `definition` is defined, reaching `callee`.
    pub(crate) fn define_path(
        &mut self,
        definition: proto::CallPathDefinition,
        callee: FunctionId,
        visible_caller: Option<FunctionId>,
    ) {
        let id = definition.call_path_id;
        if self.paths.contains_key(&id) {
            return;
        }
        let owner = definition.thread_id;
        let thread = self.threads.entry(owner).or_insert_with(|| {
            // A thread not defined (yet, or any more): a placeholder that
            // goes if no definition comes.
            self.provisional.push_back((self.seals, owner));
            Thread::default()
        });
        thread.paths.push(id);
        self.paths.insert(
            id,
            Path {
                definition,
                callee,
                visible_caller,
            },
        );
        // Threads spawned here before it was defined: the owner keeps them.
        for spawned in self.unlinked.remove(&id).unwrap_or_default() {
            if spawned == owner {
                continue;
            }
            if let Some(thread) = self.threads.get_mut(&spawned)
                && thread.spawner.is_none()
            {
                thread.spawner = Some(owner);
                self.threads.entry(owner).or_default().spawned += 1;
            }
        }
    }

    /// Thread `definition` is defined, with its clock epoch's.
    pub(crate) fn define_thread(
        &mut self,
        definition: proto::ThreadDefinition,
        epoch: impl FnOnce() -> proto::ClockEpochDefinition,
    ) {
        self.epochs
            .entry(definition.clock_epoch_id)
            .or_insert_with(epoch);
        let id = definition.thread_id;
        let spawner = self
            .paths
            .get(&definition.spawn_call_path_id)
            .map(|path| path.definition.thread_id)
            .filter(|&spawner| spawner != id);
        let thread = self.threads.entry(id).or_default();
        if thread.definition.is_some() {
            return;
        }
        // A placeholder that expired is a thread that arrived late: it lives
        // by the rules of a defined thread from now on, with its paths back.
        thread.finished = false;
        thread.retired = false;
        if let Some(paths) = self.expired.remove(&id) {
            self.expired_order.retain(|&expired| expired != id);
            for (path_id, path) in paths {
                if let std::collections::hash_map::Entry::Vacant(entry) = self.paths.entry(path_id)
                {
                    entry.insert(path);
                    thread.paths.push(path_id);
                }
            }
        }
        let spawn = definition.spawn_call_path_id;
        thread.definition = Some(definition);
        thread.spawner = spawner;
        if let Some(spawner) = spawner {
            self.threads.entry(spawner).or_default().spawned += 1;
        } else if spawn != 0 && !self.paths.contains_key(&spawn) {
            self.unlinked.entry(spawn).or_default().push(id);
        }
    }

    /// Thread `id` is defined.
    pub(crate) fn has_thread(&self, id: u64) -> bool {
        self.threads
            .get(&id)
            .is_some_and(|thread| thread.definition.is_some())
    }

    /// Thread `id` completed in the open file.
    pub(crate) fn thread_finished(&mut self, id: u64) {
        self.finished.push(id);
    }

    /// The open file references function `id`.
    pub(crate) fn function(&mut self, id: FunctionId, out: &mut Out<'_>) {
        if !self.file.functions.insert(id) {
            return;
        }
        let (definition, full) = out.functions.definition(id);
        if full {
            push(
                &mut out.definitions.functions,
                out.bytes,
                field::definitions::FUNCTIONS,
                definition,
            );
        } else {
            push(
                &mut out.definitions.slim_functions,
                out.bytes,
                field::definitions::SLIM_FUNCTIONS,
                definition,
            );
        }
    }

    /// The open file references call path `id` (0 for none).
    #[inline]
    pub(crate) fn path(&mut self, id: u32, out: &mut Out<'_>) {
        if id == 0 || id == self.file.last_path {
            return;
        }
        if self.file.paths.contains(&id) {
            self.file.last_path = id;
            return;
        }
        // All or nothing: a path is written with every path above it and
        // their threads, or not yet.
        if !self.path_ready(id, 0) {
            self.file.missing_paths.insert(id);
            return;
        }
        self.new_path(id, out);
    }

    /// A path not yet in the open file: it and every path above it, with the
    /// functions and the thread each names.
    #[cold]
    #[inline(never)]
    fn new_path(&mut self, first: u32, out: &mut Out<'_>) {
        let mut id = first;
        while id != 0 && !self.file.paths.contains(&id) {
            let Some(path) = self.paths.get(&id) else {
                self.file.missing_paths.insert(id);
                break;
            };
            let (definition, callee, visible_caller) =
                (path.definition, path.callee, path.visible_caller);
            self.file.paths.insert(id);
            push(
                &mut out.definitions.call_paths,
                out.bytes,
                field::definitions::CALL_PATHS,
                definition,
            );
            self.function(callee, out);
            if let Some(caller) = visible_caller {
                self.function(caller, out);
            }
            self.thread(definition.thread_id, out);
            id = definition.parent_call_path_id;
        }
        if self.file.paths.contains(&first) {
            self.file.last_path = first;
        }
    }

    /// The open file references thread `id`.
    #[inline]
    pub(crate) fn thread(&mut self, id: u64, out: &mut Out<'_>) {
        if id == self.file.last_thread {
            return;
        }
        if self.file.threads.contains(&id) {
            self.file.last_thread = id;
            return;
        }
        // All or nothing: a thread is written with its spawn path, or not yet.
        if !self.thread_ready(id, 0) {
            self.file.missing_threads.insert(id);
            return;
        }
        self.new_thread(id, out);
    }

    #[cold]
    #[inline(never)]
    fn new_thread(&mut self, id: u64, out: &mut Out<'_>) {
        let Some(definition) = self
            .threads
            .get(&id)
            .and_then(|thread| thread.definition.clone())
        else {
            self.file.missing_threads.insert(id);
            return;
        };
        self.file.threads.insert(id);
        self.file.last_thread = id;
        let (epoch, spawn) = (definition.clock_epoch_id, definition.spawn_call_path_id);
        push(
            &mut out.definitions.threads,
            out.bytes,
            field::definitions::THREADS,
            definition,
        );
        self.epoch(epoch, out);
        self.path(spawn, out);
    }

    /// The open file references clock epoch `id`: its definition and its
    /// latest observed state.
    pub(crate) fn epoch(&mut self, id: u64, out: &mut Out<'_>) {
        if self.file.epochs.contains(&id) {
            return;
        }
        if let Some(definition) = self.epochs.get(&id) {
            self.file.epochs.insert(id);
            push(
                &mut out.definitions.clock_epochs,
                out.bytes,
                field::definitions::CLOCK_EPOCHS,
                *definition,
            );
            if let Some(state) = self.states.get(&id) {
                push(
                    out.clock_states,
                    out.bytes,
                    field::clock_state_batch::STATES,
                    *state,
                );
            }
        }
    }

    /// Epoch `state.epoch_id` was observed in `state`, which the open file
    /// carries, with the epoch's definition if it is the first reference.
    pub(crate) fn observe(&mut self, state: proto::ClockEpochState, out: &mut Out<'_>) {
        let id = state.epoch_id;
        self.states.insert(id, state);
        if self.file.epochs.contains(&id) || !self.epochs.contains_key(&id) {
            push(
                out.clock_states,
                out.bytes,
                field::clock_state_batch::STATES,
                state,
            );
        } else {
            self.epoch(id, out);
        }
    }

    /// Call path `id` and every path above it are defined, with the
    /// threads that defined them ready.
    pub(crate) fn complete(&mut self, id: u32) -> bool {
        self.path_ready(id, 0)
    }

    /// A record of thread `thread` (0 for none) at call path `path` (0 for
    /// none) can be written: every definition it needs is known.
    #[inline]
    pub(crate) fn ready(&mut self, thread: u64, path: u32) -> bool {
        (thread == 0 || self.thread_ready(thread, 0)) && self.path_ready(path, 0)
    }

    /// How deep readiness follows spawns: past this, a chain is not ready.
    const READY_DEPTH: u32 = 256;

    #[inline]
    fn path_ready(&mut self, id: u32, depth: u32) -> bool {
        if id == 0 || id == self.last_ready_path {
            return true;
        }
        if self.ready_paths.contains(&id) {
            self.last_ready_path = id;
            return true;
        }
        self.check_path(id, depth)
    }

    #[cold]
    #[inline(never)]
    fn check_path(&mut self, first: u32, depth: u32) -> bool {
        if depth > Self::READY_DEPTH {
            return false;
        }
        let mut id = first;
        while id != 0 && !self.ready_paths.contains(&id) {
            let Some(path) = self.paths.get(&id) else {
                return false;
            };
            let (thread, parent) = (
                path.definition.thread_id,
                path.definition.parent_call_path_id,
            );
            if !self.thread_ready(thread, depth + 1) {
                return false;
            }
            id = parent;
        }
        // Every path from `first` up is ready.
        let mut id = first;
        while id != 0 && self.ready_paths.insert(id) {
            id = self.paths[&id].definition.parent_call_path_id;
        }
        self.last_ready_path = first;
        true
    }

    /// Thread `id` is defined and its spawn path ready.
    #[inline]
    fn thread_ready(&mut self, id: u64, depth: u32) -> bool {
        if id == self.last_ready_thread {
            return true;
        }
        // A thread spawned at a path of its own (a runtime root such as the
        // collector's) is ready once it is defined: the cycle is its own.
        if self.checking.contains(&id) {
            return self.has_thread(id);
        }
        self.checking.push(id);
        let ready = self.check_thread(id, depth);
        self.checking.pop();
        ready
    }

    fn check_thread(&mut self, id: u64, depth: u32) -> bool {
        let Some(spawn) = self
            .threads
            .get(&id)
            .and_then(|thread| thread.definition.as_ref())
            .map(|definition| definition.spawn_call_path_id)
        else {
            return false;
        };
        let ready = self.path_ready(spawn, depth);
        if ready {
            self.last_ready_thread = id;
        }
        ready
    }

    /// Before the open file seals: what it referenced before its definition
    /// arrived, now defined. Returns the paths still missing.
    pub(crate) fn resolve_missing(&mut self, out: &mut Out<'_>) -> FxHashSet<u32> {
        for id in std::mem::take(&mut self.file.missing_threads) {
            self.thread(id, out);
        }
        // A path defined before its thread was: its thread now.
        let threads: Vec<u64> = (self.file.paths.iter())
            .filter_map(|id| self.paths.get(id))
            .map(|path| path.definition.thread_id)
            .collect();
        for id in threads {
            self.thread(id, out);
        }
        for id in std::mem::take(&mut self.file.missing_paths) {
            self.path(id, out);
        }
        std::mem::take(&mut self.file.missing_paths)
    }

    /// The open file sealed at `now`: the next references nothing yet. Each
    /// thread that finished in it starts its grace; one whose grace ran out
    /// goes once nothing keeps it, unless `in_use` (an open request it made)
    /// still needs it.
    pub(crate) fn seal(&mut self, now: Instant, in_use: impl Fn(u64) -> bool) {
        let file = &mut self.file;
        file.functions.clear();
        file.paths.clear();
        file.threads.clear();
        file.epochs.clear();
        file.missing_paths.clear();
        file.missing_threads.clear();
        file.last_path = 0;
        file.last_thread = 0;
        self.seals += 1;
        let seals = self.seals;
        let wait = self.grace_seals.unwrap_or(u64::from(HOLD_SEALS) + 1);
        for id in self.finished.drain(..) {
            if let Some(thread) = self.threads.get_mut(&id) {
                thread.finished = true;
                self.retiring.push_back((now, seals, id));
            }
        }
        while let Some(&(at, sealed, id)) = self.retiring.front() {
            if now.duration_since(at) < self.grace.unwrap_or(GRACE) || seals - sealed < wait {
                break;
            }
            self.retiring.pop_front();
            if in_use(id) {
                self.retiring.push_back((now, seals, id));
                continue;
            }
            if let Some(thread) = self.threads.get_mut(&id) {
                thread.retired = true;
            }
            self.forget(id);
        }
        // A placeholder whose thread was never defined: its paths go.
        while let Some(&(made, id)) = self.provisional.front() {
            if seals - made < wait {
                break;
            }
            self.provisional.pop_front();
            if let Some(thread) = self.threads.get_mut(&id)
                && thread.definition.is_none()
            {
                thread.finished = true;
                thread.retired = true;
                self.forget(id);
            }
        }
    }

    /// Thread `first` goes if it is retired and keeps no spawned thread,
    /// and with it each spawner it was the last to keep.
    fn forget(&mut self, first: u64) {
        let mut next = Some(first);
        while let Some(id) = next.take() {
            let Some(thread) = self.threads.get(&id) else {
                return;
            };
            if !thread.retired || thread.spawned > 0 {
                return;
            }
            let thread = self.threads.remove(&id).expect("present");
            let undefined = thread.definition.is_none();
            let mut kept = Vec::new();
            for path in thread.paths {
                if let Some(definition) = self.paths.remove(&path)
                    && undefined
                {
                    kept.push((path, definition));
                }
            }
            // A thread never defined may yet be: its paths wait for it.
            if undefined && !kept.is_empty() {
                if self.expired_order.len() == EXPIRED_THREADS
                    && let Some(oldest) = self.expired_order.pop_front()
                {
                    self.expired.remove(&oldest);
                }
                self.expired.insert(id, kept);
                self.expired_order.push_back(id);
            }
            // Readiness found through these paths, for any thread's path, is
            // no longer true: found again as needed.
            self.ready_paths.clear();
            self.last_ready_path = 0;
            self.last_ready_thread = 0;
            if thread.spawner.is_none()
                && let Some(spawn) = thread
                    .definition
                    .as_ref()
                    .map(|definition| definition.spawn_call_path_id)
                && let Some(waiting) = self.unlinked.get_mut(&spawn)
            {
                waiting.retain(|&waiting| waiting != id);
                if waiting.is_empty() {
                    self.unlinked.remove(&spawn);
                }
            }
            if let Some(spawner) = thread.spawner
                && let Some(kept) = self.threads.get_mut(&spawner)
            {
                kept.spawned -= 1;
                next = Some(spawner);
            }
        }
    }

    /// Threads, and call paths, whose definitions are kept.
    #[cfg(test)]
    pub(crate) fn kept(&self) -> (usize, usize) {
        (self.threads.len(), self.paths.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn function(n: u64) -> FunctionId {
        let allocator = btel_types::FunctionIdAllocator::default();
        let mut id = allocator.allocate().unwrap();
        for _ in 1..n {
            id = allocator.allocate().unwrap();
        }
        id
    }

    fn thread(id: u64, spawn: u32) -> proto::ThreadDefinition {
        proto::ThreadDefinition {
            thread_id: id,
            spawn_call_path_id: spawn,
            clock_epoch_id: 9,
            ..proto::ThreadDefinition::default()
        }
    }

    fn epoch() -> proto::ClockEpochDefinition {
        proto::ClockEpochDefinition {
            epoch_id: 9,
            ..proto::ClockEpochDefinition::default()
        }
    }

    fn path(id: u32, parent: u32, thread: u64, callee: FunctionId) -> proto::CallPathDefinition {
        proto::CallPathDefinition {
            call_path_id: id,
            thread_id: thread,
            parent_call_path_id: parent,
            callee_function_id: callee.get(),
            ..proto::CallPathDefinition::default()
        }
    }

    struct Sink {
        definitions: proto::Definitions,
        clock_states: Vec<proto::ClockEpochState>,
        bytes: usize,
        functions: FunctionDefinitions,
    }

    impl Sink {
        fn new() -> Self {
            Self {
                definitions: proto::Definitions::default(),
                clock_states: Vec::new(),
                bytes: 0,
                functions: FunctionDefinitions::default(),
            }
        }
        fn out(&mut self) -> Out<'_> {
            Out {
                definitions: &mut self.definitions,
                clock_states: &mut self.clock_states,
                bytes: &mut self.bytes,
                functions: &mut self.functions,
            }
        }
        fn take(&mut self) -> proto::Definitions {
            self.bytes = 0;
            std::mem::take(&mut self.definitions)
        }
    }

    fn ids(definitions: &proto::Definitions) -> (Vec<u32>, Vec<u64>, Vec<u64>, usize) {
        let mut paths: Vec<u32> = definitions
            .call_paths
            .iter()
            .map(|p| p.call_path_id)
            .collect();
        paths.sort_unstable();
        let mut threads: Vec<u64> = definitions.threads.iter().map(|t| t.thread_id).collect();
        threads.sort_unstable();
        let mut epochs: Vec<u64> = definitions
            .clock_epochs
            .iter()
            .map(|e| e.epoch_id)
            .collect();
        epochs.sort_unstable();
        let functions = definitions.functions.len() + definitions.slim_functions.len();
        (paths, threads, epochs, functions)
    }

    #[test]
    fn a_path_brings_every_path_above_it_and_what_they_name_into_each_file() {
        let (f, g) = (function(1), function(2));
        let mut defs = FileDefinitions::default();
        let mut sink = Sink::new();
        defs.define_thread(thread(1, 0), epoch);
        defs.define_path(path(5, 0, 1, f), f, None);
        defs.define_path(path(6, 5, 1, g), g, Some(f));
        defs.path(6, &mut sink.out());
        defs.path(6, &mut sink.out());
        defs.path(5, &mut sink.out());
        let first = sink.take();
        assert_eq!(ids(&first), (vec![5, 6], vec![1], vec![9], 2));
        defs.seal(Instant::now(), |_| false);
        // The next file repeats them: it is read alone.
        defs.path(6, &mut sink.out());
        assert_eq!(ids(&sink.take()), (vec![5, 6], vec![1], vec![9], 2));
        defs.seal(Instant::now(), |_| false);
        // Nothing referenced, nothing defined.
        assert_eq!(ids(&sink.take()), (vec![], vec![], vec![], 0));
    }

    #[test]
    fn a_future_brings_its_spawners_paths() {
        let f = function(1);
        let mut defs = FileDefinitions::default();
        let mut sink = Sink::new();
        defs.define_thread(thread(1, 0), epoch);
        defs.define_path(path(5, 0, 1, f), f, None);
        defs.define_path(path(7, 5, 1, f), f, None);
        defs.define_thread(thread(2, 7), epoch);
        defs.thread(2, &mut sink.out());
        assert_eq!(ids(&sink.take()), (vec![5, 7], vec![1, 2], vec![9], 1));
    }

    #[test]
    fn a_finished_thread_goes_after_its_grace_once_nothing_keeps_it() {
        let f = function(1);
        // Time alone here; the seals a finished thread waits are tested apart.
        let mut defs = FileDefinitions {
            grace_seals: Some(0),
            ..FileDefinitions::default()
        };
        let start = Instant::now();
        let later = |n: u32| start + GRACE * n;
        // Root 1 defines path 5; future 2 spawned at it defines 6; future 3
        // spawned at 6 defines 7; future 4, spawned at 5, made a request.
        defs.define_thread(thread(1, 0), epoch);
        defs.define_path(path(5, 0, 1, f), f, None);
        defs.define_thread(thread(2, 5), epoch);
        defs.define_path(path(6, 5, 2, f), f, None);
        defs.define_thread(thread(3, 6), epoch);
        defs.define_path(path(7, 6, 3, f), f, None);
        defs.define_thread(thread(4, 5), epoch);
        assert_eq!(defs.kept(), (4, 3));
        // 2 finishes first: 3 still runs under it, so it stays.
        defs.thread_finished(2);
        defs.seal(later(0), |_| false);
        defs.seal(later(1), |_| false);
        assert_eq!(defs.kept(), (4, 3));
        // 3 and 4 finish; 4's request is still open.
        defs.thread_finished(3);
        defs.thread_finished(4);
        defs.seal(later(2), |id| id == 4);
        // Within the grace: kept for the aggregates still to come.
        defs.seal(later(2), |id| id == 4);
        assert_eq!(defs.kept(), (4, 3));
        // 3 goes, and 2 with it; 4 waits for its request.
        defs.seal(later(3), |id| id == 4);
        assert_eq!(defs.kept(), (2, 1));
        defs.seal(later(5), |_| false);
        assert_eq!(defs.kept(), (1, 1));
        assert!(defs.has_thread(1));
    }

    #[test]
    fn a_reference_before_its_definition_resolves_at_seal() {
        let f = function(1);
        let mut defs = FileDefinitions::default();
        let mut sink = Sink::new();
        defs.path(5, &mut sink.out());
        defs.thread(1, &mut sink.out());
        defs.define_thread(thread(1, 0), epoch);
        defs.define_path(path(5, 0, 1, f), f, None);
        assert!(defs.resolve_missing(&mut sink.out()).is_empty());
        assert_eq!(ids(&sink.take()), (vec![5], vec![1], vec![9], 1));
        // Never defined: still missing.
        defs.path(8, &mut sink.out());
        assert_eq!(
            defs.resolve_missing(&mut sink.out())
                .into_iter()
                .collect::<Vec<_>>(),
            [8]
        );
    }

    #[test]
    fn a_thread_spawned_at_its_own_path_is_ready_once_defined() {
        let f = function(1);
        let mut defs = FileDefinitions::default();
        // The collector: a root whose spawn path is a path of its own.
        defs.define_path(path(12, 0, 6, f), f, None);
        assert!(!defs.ready(6, 12));
        defs.define_thread(thread(6, 12), epoch);
        assert!(defs.ready(6, 12));
        assert!(defs.ready(6, 0));
    }

    #[test]
    fn a_path_of_a_thread_never_defined_goes_after_the_hold_window() {
        let f = function(1);
        let mut defs = FileDefinitions::default();
        // A path whose thread is not defined, as when its thread was already
        // forgotten: a placeholder that must not stay for good.
        defs.define_path(path(5, 0, 1, f), f, None);
        assert_eq!(defs.kept(), (1, 1));
        let now = Instant::now();
        for _ in 0..HOLD_SEALS {
            defs.seal(now, |_| false);
        }
        assert_eq!(defs.kept(), (1, 1), "kept while a held record may need it");
        defs.seal(now, |_| false);
        assert_eq!(defs.kept(), (0, 0));
    }
}
