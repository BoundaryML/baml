// Output writes <demo>/app/baml_sdk/, so app.py can `import baml_sdk`
// directly with the demo's app/ as cwd.
generator target {
  output_type "python/pydantic"
  output_dir "../app"
}
