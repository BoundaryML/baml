## generators

This directory contains generators for various languages as well as integration tests for the generators and a test harness for running those tests.
The generator for each language generates types/functions from the IR.

## Directory structure

 - `data/` contains the data for the generators.
 - `languages/` contains the generators for various languages.
 - `utils/` contains the test harness for running tests.

## Steps to add a new integration test

```sh
cd utils/test_harness/
```

To add a new test for Go:
```sh
cargo make add-go-test <test-name>
```

To add a new test for Python:
```sh
cargo make add-python-test <test-name>
```

This will create a new directory in the `data/` directory with the name of the test and setup the test files.


## Running tests

Run tests for a specific package:
```sh
RUN_GENERATOR_TESTS=1 cargo test --package <package_name>
```

Some examples:

Run tests for the Go language:
```sh
RUN_GENERATOR_TESTS=1 cargo test --package generators-go
```

Run tests for the `classes` module in the Go language:
```sh
RUN_GENERATOR_TESTS=1 cargo test --package generators-go --lib -- classes
```


> [!CAUTION]
> Do not run tests while you're inside a set of tests directory, it might break your shell.
> This happens because the test harness cleans up the directories inside the `generated_tests` directory.

