Run the tests like this:

infisical run --env=test -- poetry run pytest app/test_functions.py

env -u CONDA_PREFIX poetry run maturin develop --manifest-path ../../engine/language_client_python/Cargo.toml && poetry run baml-cli generate --from ../baml_src

BAML_LOG=baml_events infisical run --env=test -- poetry run pytest app/test_functions.py -s

