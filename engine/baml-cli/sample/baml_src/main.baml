// All baml projects require a main.baml file in the root directory
// Tip: Make sure you've got the VSCode extension for .baml files installed

generator default {
  // Right now only python is supported, but soon TS
  language python
  pkg_manager poetry
}

// Add your functions down here from the tutorials in 
// https://docs.boundaryml.com/v3/guides/hello_world/level0