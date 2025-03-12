# BAML Ruby Integration Tests

Install `mise` to manage ruby installations

1. Build the ruby FFI client
`cd ../engine/language_client_ruby`
`cargo build`
`mise exec -- bundle install`
`mise exec -- rake compile`

to speed it up, you can try building the dev mode before doing rake compile:
`export RB_SYS_CARGO_PROFILE="dev"` 
   
## Running Tests

In this directory (integ-tests/ruby)

Install deps
```bash
mise exec -- bundle install
```

Generate the BAML client code:
```bash
mise exec -- baml-cli generate --from ../baml_src
```


### Run all tests
```bash
infisical run --env=test -- mise exec -- rake test
```

### Run specific tests
```bash
# Run a specific test file
infisical run --env=test -- mise exec -- ruby test_functions.rb

# Run a specific test
```bash
infisical run --env=test -- mise exec -- rake test test_collector.rb TEST_OPTS="--name=/test_collector_no_stream_success/ -v"
```

### Environment Variables
- Tests can be run with environment variables using `infisical` (default)
```bash
infisical run --env=test -- mise exec -- rake test
```

- Alternatively, you can use a .env file with dotenv:
```bash
mise exec -- rake test
```

## Project Structure

- `baml_client/` - Generated BAML client code
- `test_functions.rb` - Main test file
- `streaming-example.rb` - Streaming functionality examples
- `tracing-demo1.rb` - Tracing functionality examples
- `Gemfile` - Ruby dependencies
- `Rakefile` - Test and build tasks

## Debugging Tests
### Debug Logs
- Add `puts` statements in your tests
- Set the environment variable `BAML_LOG=trace` for detailed BAML client logs:
```bash
BAML_LOG=trace infisical run --env=test -- mise exec -- rake test
```

## Troubleshooting

### Common Issues

1. **Missing API Keys**
   - Ensure all required API keys are set in your environment
   - Check that `.env` file exists if not using Infisical
   - Verify Infisical is properly configured if using `infisical run`

2. **Build Issues**
   - If you get Rust compilation errors:
     ```bash
     # Clean and rebuild
     (cd ../../engine/language_client_ruby && mise exec -- rake clean compile)
     ```
   - For Bundler issues:
     ```bash
     mise exec -- bundle install --clean
     ```

3. **Ruby Version Issues**
   - Ensure mise is properly set up:
     ```bash
     mise install
     mise exec -- ruby --version
     ```
   - If mise isn't picking up the right version:
     ```bash
     mise trust
     mise install
     ```

4. **BAML Client Generation Issues**
   - Ensure BAML CLI is up to date
   - Check that BAML source files in `../baml_src` are valid
   - Try regenerating the client:
     ```bash
     rm -rf baml_client
     mise exec -- baml-cli generate --from ../baml_src
     ```

5. **Test Load Path Issues**
   - If tests can't find files, ensure you're running from the correct directory
   - Try running with full paths:
     ```bash
     mise exec -- ruby -I. test_functions.rb
     ```

### Getting Help
- Run tests with verbose output:
  ```bash
  infisical run --env=test -- mise exec -- rake test TESTOPTS="--verbose"
  ```
- Use Ruby's debug mode:
  ```bash
  infisical run --env=test -- mise exec -- ruby -rdebug test_functions.rb
  ```
- Check the test output for error backtraces and assertion details
