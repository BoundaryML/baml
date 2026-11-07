# BAML runtime heap

Single-owner, non-moving mark-region collector for native baml_rt.
Objects have an eight-byte descriptor header; marks live in side metadata.
Small allocations use 32 KiB blocks, 128-byte lines and 16-byte alignment;
objects above 8 KiB are allocated and swept separately. Empty storage is reused.
Allocation returns zeroed storage and publishes object starts immediately.

`RootSource` supplies conservative word ranges and precise pointer slots.
The caller must spill registers, keep stacks readable and fibers stopped during
collection, and root managed values held in host allocations explicitly.
Interior candidates are checked against requested object size, excluding padding.
Heap edges are traced precisely using immutable descriptors and pointer-type tags.
Use `write_ptr` for pointer stores; descriptor IDs must not change after allocation.
Unsafe APIs require complete roots and prohibit use after reclamation.

Collection budget is max(32 MiB, four times live bytes). `try_alloc` performs a
full collection/retry before failure. Capacity limits govern usable managed
storage, not RSS; backing batches and host bookkeeping are separate.
Finalizers retain their reachable graph until draining. External payload memory
needs explicit accounting. There is no compaction or cross-thread allocation.

From `baml_language`, with `CARGO_TARGET_DIR` inside your worktree:

```sh
cargo test -p baml_rt_gc
cargo test -p baml_rt_gc --release
cargo run -p baml_rt_gc --release --example bench
```

The fiber test keeps eight objects alive across ten parked-stack collections.
The benchmark uses precise roots to isolate allocation and tracing costs.
Design notes and results are in the baml_rt PR description (#5566).
