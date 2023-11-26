// All baml projects require a main.baml file in the root directory

// Tip: Make sure you've got the VSCode extension for .baml files installed

generator default {
  // Right now only python is supported, but soon TS
  language python

  // Uncomment this line if you use poetry
  pkg_manager poetry
}
