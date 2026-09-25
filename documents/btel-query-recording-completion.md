# Recording completion and final clocks

A recording now says when it is over. After a normal shutdown, its last file
carries `RecordingEnd`, and every clock the recording used is marked final.
`baml query` then reports the recording as `sealed`. This is the
[demo](btel-query-demo.md) project after its last step:

```text
$ baml query "SELECT state, seal_state, indexed_sequence, terminal_sequence FROM recordings ORDER BY terminal_sequence"
state  | seal_state | indexed_sequence | terminal_sequence
-------+------------+------------------+------------------
gap    | unsealed   | 2                | NULL
sealed | sealed     | 1                | 1
sealed | sealed     | 1                | 1
sealed | sealed     | 1                | 1
```

The three `baml run` recordings are sealed. Each short run fit in one file,
so that file also carries the end. The demo deleted file 3 of the fourth
recording on purpose. Its end marker is in file 7, beyond the gap, so the
reader does not apply it and the recording shows as a `gap`.

Both fields already existed on the wire. Until now the producer never wrote
them, so every recording read as `unsealed` and every clock as not final.

## What the end marker means

The last file carries `RecordingEnd` only when all of this is true:

1. No more telemetry can arrive. Admission is closed, no producer is still
   writing, and the processor has consumed every published chunk.
2. Every clock epoch the recording observed has settled (next section).
3. The recording was not already failed when the end was due. A producer
   failure, or delivery that was already disabled, suppresses the end.

Its sequence number is the number of files in the recording. The marker says
"there are no more files". It does not say every file or blob arrived, and it
says nothing about whether each run succeeded. Losses that happen after the
end was produced can coexist with it. A cloud upload of an earlier file can
fail after the end file was already uploaded; that file then shows as a gap.

When input is exhausted but an epoch is still unsettled, the recorder still
writes the pending data. It adds each unsettled epoch's latest status, marked
not final, and leaves out the end. The recording stays `unsealed`. This keeps
the existing wire contract, which says the end comes only after "input,
metadata attempts and final epoch states settle".

Flushing is not ending. Size and time rotation, and forced flushes, never
write the marker; later input continues the sequence. The builder method that
used to be called `finish_recording` was only a forced flush, so it is now
`flush_recording`. The terminal step is a separate `end_recording`:

- Called on an empty recording, it writes file 1 with only the header and the
  end.
- Called after the last data was already flushed, it writes one extra file
  that carries only the end (and any final clock states).
- Called again after the end, it writes nothing.
- Input that arrives after the end is rejected with `InputAfterEnd`, by both
  `flush_recording` and `end_recording`.

`Publisher::finish` is the only caller in production. The processor calls it
once, when the ring buffer reports input exhausted. `Publisher::flush` still
only flushes.

## When a clock is final

Each run gets its own clock epoch, and its spawned threads share it. The epoch
already counted attached threads, so restores and mode changes could skip
finished runs. `ClockEpoch::settled_status()` exposes that same fact: it
returns the status once every thread attached to the epoch has finished.

One finished thread is not enough. If a run's root returns while a spawned
child keeps running, the epoch is not settled and its clock is not final.

A settled run cannot gain threads. The engine builds a child's telemetry state
while the parent is still attached, so the count cannot drop to zero and then
rise again.

A settled status cannot change. Status only ever moves from valid to invalid.
Restores, mode changes and fault fallbacks invalidate unsettled runs only.
They now make that decision under each epoch's own lock, and
`settled_status()` reads the status under the same lock. Without that lock, a
restore that started before the last thread finished could still invalidate a
run the recorder had already reported final. The test
`a_settled_status_survives_restores_racing_the_last_thread` repeats that race.

The VM writes a thread's definition when the thread completes, right before
it detaches from the epoch. The recorder reads the epoch's status when it
converts that definition. Usually the run has settled by then, so the final
state goes into the same file and the recorder keeps nothing. Otherwise it
keeps the epoch in a small map and checks again whenever it seals a file, and
once more at the end. An epoch leaves the map as soon as its final state is
written.

So the map holds only runs that are still going, plus runs that settled since
the last file. Short root calls that settle before conversion never enter it.
A spawned thread that never finishes keeps its epoch in the map until the
recording ends.

## Delivery order

Local recordings use one writer queue. The terminal file is queued after
every earlier file and CAS blob, and the writer writes them in order. Engine
shutdown drains the processor first and the writer second. If a write fails,
recording is disabled and the end may be missing, which reads as `unsealed`.

Cloud recordings submit the terminal file after every earlier file, but
uploads run concurrently. An earlier file can still be dropped for capacity
or fail its upload, and so can a blob. The end then shows how many files to
expect, and the missing sequence stays visible as a `gap`. The reader does not
apply an end that lies beyond a missing file, so such a recording is not
reported `sealed` until the file appears. Delivery loss counts
(`telemetry_delivery_loss_count`) are not written into the recording.

## Engine lifecycle

`shutdown_with_deadline` waits for active calls and spawned futures, runs the
final GC, drains the processor, then drains delivery. The end is written
while the processor drains.

| Situation | Result |
|---|---|
| Normal shutdown, every thread finished | Sealed; every recorded clock final |
| Engine dropped without `shutdown` | Same rules, once the last VM releases the telemetry runtime |
| Shutdown deadline abandons a spawned thread | Unsealed; that run's clock written with its latest status, not final |
| Crash or killed process before the end file was written | No end |
| Crash after the end file was written | Still sealed |
| `BAML_TELEMETRY=off` | No recording at all |
| Producer failure, or delivery already disabled, before the end was produced | No end |
| Local write fails before the end file is written | No end on disk; the writer stops at the failed file |
| An earlier cloud file or blob is lost after the end was produced | The end can still arrive; the lost file is a gap and the lost blob a missing value |
| A root call future dropped before its thread completed | That thread has no definition, so its epoch is never observed and the recording can still end. The reader reports the thread as unresolved |

`telemetry_result() == Some(Ok(()))` still does not say whether the end was
written. A run that was abandoned is not a telemetry failure.

## Reading it

- `recordings.state` is `sealed` when the end file and every earlier file were
  applied. `seal_state` is `sealed` whenever the end file was applied.
- `clocks.is_final` is 1 for settled runs. `timing_state` keeps its values;
  `valid` with `is_final = 1` can no longer be invalidated.
- The `capabilities` row "sealed recordings and final clocks" is now
  `supported`.
- Run completion stays separate. `executions.status` still comes from the
  root thread's completion. A sealed recording can contain an `incomplete`
  execution (the dropped root call above); no completion will arrive for it.
- The playground shows run completion and index gaps, never the seal state,
  so it needed no change.

Older recordings have no end and no final states. They read exactly as
before: `unsealed`, `is_final = 0`.

The regenerated [demo](btel-query-demo.md) shows the difference live. While
`slow` runs, its recording is `unsealed`; after it exits, it is `sealed`:

```text
rows: [[4, 'unsealed', 'incomplete', 3728]] | files decoded: 2 unchanged: 5
(slow finished)
rows: [[7, 'sealed', 'ok', 5593]] | files decoded: 3 unchanged: 7
```

## What did not change

The VM, engine sources, heap, event records, snapshot and CAS code, and the
chunk transport are untouched. So are frame layouts and all per-call and
per-instruction work. The new work runs on the processor thread, once per
thread definition and once per sealed file, plus the shutdown path.

The clock crate changed in two cold places. `settled_status` is new and only
the recorder calls it. `invalidate_active`, which runs on restore, mode change
and fault fallback, now holds each epoch's validation lock while it decides.
The epoch's layout and `read`, `attach_thread`, `finish_thread` and `check`
are unchanged.

Producer audit, `documents/check-producer-baseline.py`:

- Outcome-phase manifest (120 files): exactly two files differ,
  `btel_clock/src/lib.rs` and `btel_clock/src/tests.rs`. The manifest was not
  regenerated.
- Historical 164-file manifest: besides the 8 files already different at
  `0735e3909`, these now differ: `btel_clock/src/{lib,tests}.rs`,
  `btel_recorder/src/{clock,lib,recording}.rs`,
  `btel_processor/src/publisher.rs`, `btel_file/src/{lib,publisher}.rs` and
  `btel_bcs/src/{metadata,publisher}.rs`. All but the clock files are the
  background recorder, processor and delivery code that the outcome phase
  already authorized.

## Tests

Validation on Rust 1.98.0: 222 tests passed, none failed, and one existing
fixture-generation test stayed ignored. Formatting and Clippy with warnings
denied passed on all targets of the eight checked crates. The real CLI demo
also passed. No full-workspace build or interactive playground check was run.

- Clock: `only_runs_whose_threads_all_finished_report_a_settled_status`
  (child still attached, invalid run still attached, restore after
  settlement) and `a_settled_status_survives_restores_racing_the_last_thread`.
- Recorder: `flush_continues_the_recording_and_end_seals_it_once`,
  `an_empty_recording_ends_with_a_header_only_file`,
  `already_sealed_data_is_followed_by_an_end_only_file`,
  `input_after_the_end_is_rejected_by_both_flush_and_end`,
  `an_unsettled_clock_keeps_the_recording_unsealed_until_it_settles`,
  `clock_bookkeeping_holds_only_unsettled_runs_and_releases_them_once_final`,
  and `processor_ends_the_recording_only_after_producers_stop_writing`, where
  two producers keep writing after admission closes.
- Local and cloud: `local_recording_ends_after_its_flushed_data_and_only_once`;
  the cloud publisher tests check the terminal upload, an end-only upload
  after three sealed windows, and no end after a delivery failure.
- Engine: `telemetry_files` and `cloud_telemetry` check that a real shutdown
  seals the recording, the end is on the last file only, and every recorded
  epoch is final.
- Query: `end_markers_and_final_clocks_are_evidence_and_older_recordings_stay_unsealed`
  covers an ended recording, an older one and an end beyond a missing file.
  The engine acceptance tests and the CLI `query_e2e` test check that real
  `baml run` recordings come back `sealed` with every clock final.

## Limits

- Sealing is per recording. One abandoned spawned thread leaves the whole
  recording `unsealed`, including runs that finished.
- A final state for a run that settles after its last completion was
  converted waits for the next file that is sealed anyway. An idle engine
  writes it at the next activity or at the end.
- Observations are not deduplicated. A file can carry the same epoch twice;
  the reader merges them.
- Finality relies on the engine's lifecycle: children attach while their
  parent is attached, and no VM validates its clock after its thread
  finished. The clock API itself does not forbid attaching to a settled
  epoch.
- An unsealed recording is still not proof that the program is running.
  There is no liveness record.

## Performance and remaining parity

The [comparison with unmodified upstream](btel-query-upstream-performance.md)
measures all producer changes in the query branch, including this one, in
108 seconds of paired runs. The
[remaining-question audit](btel-query-remaining-questions.md) lists the
old-tracer questions still unsupported or only partially answered.
