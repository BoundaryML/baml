# Getting started with BAML

## Installations

Make sure to download the [VSCode Playground](https://marketplace.visualstudio.com/items?itemName=gloo.baml).

To use BAML with either python or typescript, you should run:

```shell
$ baml update-client
```

This will keep client side libraries in sync. It also prints the commands being run, which you can run manually if they fail.

## Running tests

You can run tests via:

```shell
# To run tests
$ baml test run

# To list tests
$ baml test

# For more help
$ baml test --help
```

## Integrating BAML with python / ts

You can run:

```shell
$ python -m baml_example_app
```

The `baml_example_app.py` file shows how to import from the code BAML generates.

## Deploying

You don't need the BAML compiler when you deploy / release. Your `baml_client` folder contains everything you may need.

## Reporting bugs

Report any issues on our [Github](https://www.github.com/boundaryml/baml) or [Discord](https://discord.gg/BTNBeXGuaS)
