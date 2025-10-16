function WatchMissingValue() -> int {
    let progress: int = 0 @watch(skip_def=);
    progress.watchers.$notify();
    progress
}

// error: Error validating: Missing value for watch argument e.g. true, false
//   -->  expr/emit_missing_value.baml:2
//    | 
//  1 | function WatchMissingValue() -> int {
//  2 |     let progress: int = 0 @watch(skip_def=);
//    | 
