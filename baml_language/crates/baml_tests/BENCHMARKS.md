# BAML Compiler Benchmarks

This directory contains comprehensive benchmarks for the BAML compiler, measuring performance across different scenarios.

## Running Benchmarks

```bash
# Run all benchmarks
cargo bench --bench compiler_benchmark

# Run specific benchmark by name
cargo bench --bench compiler_benchmark bench_incremental_add_user_field

# Run with output
cargo bench --bench compiler_benchmark -- --nocapture
```

## Benchmark Categories

### 1. Incremental Benchmarks (`benches/incremental/`)

Measure incremental compilation performance when files change in multi-file projects.

**How it works:**
- `before/` contains the initial project state
- `after/` contains ONLY the files that changed (sparse directory)
- Benchmarks measure the time to recompile after changes

**Current benchmarks:**
- `add_user_field` - Adding fields to an existing class
- `rename_type` - Renaming a type used across files

### 2. Scale Benchmarks (`benches/scale/`)

Test compiler performance with large files.

**Current benchmarks:**
- `100_functions.baml` - File with 100 function definitions
- `deep_nesting.baml` - Deeply nested type hierarchies

### 3. Realistic Benchmarks (`benches/realistic/`)

Complete, real-world BAML projects for end-to-end performance testing.

## Adding New Benchmarks

### Adding an Incremental Benchmark

1. Create a new directory under `benches/incremental/your_benchmark/`

2. Add the initial project state to `before/`:
   ```
   benches/incremental/your_benchmark/
   ├── before/
   │   ├── models.baml
   │   ├── functions.baml
   │   └── clients.baml
   ```

3. Add ONLY changed files to `after/`:
   ```
   ├── after/
   │   └── models.baml  # Only this file changed
   ```

4. For file deletions, create an empty `.delete` marker:
   ```
   ├── after/
   │   └── old_file.baml.delete  # Empty file marking deletion
   ```

5. Add a `description.md` explaining the change

### Adding a Scale Benchmark

Simply add a `.baml` file to `benches/scale/`:
```
benches/scale/your_benchmark.baml
```

### Adding a Realistic Benchmark

Create a complete project under `benches/realistic/`:
```
benches/realistic/your_app/
├── models/
├── functions/
└── clients/
```

## Benchmark Implementation

The benchmark system uses a build script (`bench_build.rs`) to automatically discover and generate benchmarks at compile time.

**Key features:**
- Automatic discovery of benchmark files
- Sparse `after/` directories for incremental changes
- Support for file deletions via `.delete` markers
- Measures both full and incremental compilation

## Understanding Results

When you run benchmarks, you'll see output like:
```
test bench_incremental_add_user_field ... bench:       5,234 ns/iter (+/- 512)
test bench_incremental_rename_type    ... bench:      15,678 ns/iter (+/- 1,234)
test bench_scale_100_functions        ... bench:      45,123 ns/iter (+/- 3,456)
```

The time shown is per iteration. Lower is better.

## Performance Goals

- Simple incremental changes: < 10ms
- Complex refactors: < 50ms
- Large files (100+ items): < 100ms
- Full project compilation: < 200ms

## Tips for Writing Good Benchmarks

1. **Be Specific**: Each benchmark should test one specific scenario
2. **Be Realistic**: Use patterns that appear in real BAML code
3. **Document Changes**: Always include a description.md
4. **Keep After Sparse**: Only include changed files in `after/`
5. **Test Edge Cases**: Include complex scenarios like circular dependencies

## Continuous Integration

Benchmarks are run in CI to track performance over time. Significant regressions will be flagged.

## Troubleshooting

### Benchmark doesn't appear
- Ensure file has `.baml` extension
- Check that `before/` and `after/` directories exist for incremental benchmarks
- Run `cargo clean` and rebuild

### Unexpected results
- Check that `after/` only contains changed files
- Verify `.delete` markers are empty files
- Look for parse errors in benchmark files