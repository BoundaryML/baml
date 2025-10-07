function EmitMissingValue() -> int {
    let progress: int = 0 @emit(skip_def=);
    progress.$emit();
    progress
}

// error: Error validating: Missing value for emit argument e.g. true, false
//   -->  expr/emit_missing_value.baml:2
//    | 
//  1 | function EmitMissingValue() -> int {
//  2 |     let progress: int = 0 @emit(skip_def=);
//    | 
