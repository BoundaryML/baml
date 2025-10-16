"""Module for running integration tests and build commands for different project directories."""

import subprocess


def get_repo_root() -> str:
    """Get the git repository root directory."""
    result = subprocess.run(['git', 'rev-parse', '--show-toplevel'],
                          capture_output=True, text=True, check=True)
    return result.stdout.strip()


def run_all_integ_tests() -> None:
    """Run all integration tests."""
    run_python_integ_tests()
    run_typescript_integ_tests()
    run_ruby_integ_tests()


def run_python_integ_tests() -> None:
    """Run commands for the Python integration tests directory."""
    repo_root = get_repo_root()
    base_cmd = f"""
      cd {repo_root}/integ-tests/python
      uv run maturin develop --uv --manifest-path {repo_root}/engine/language_client_python/Cargo.toml
      uv run baml-cli generate --from {repo_root}/integ-tests/baml_src
      uv run pytest --capture=no
    """
    subprocess.run(base_cmd, shell=True, check=True)


def run_typescript_integ_tests() -> None:
    """Run commands for the TypeScript integration tests directory."""
    repo_root = get_repo_root()
    base_cmd = f"""
      cd {repo_root}/engine/language_client_typescript
      pnpm build:debug
      pnpm baml-cli generate --from {repo_root}/integ-tests/baml_src
      cd {repo_root}/integ-tests/typescript
      pnpm test -- --silent false --testTimeout 30000
    """
    subprocess.run(base_cmd, shell=True, check=True)


def run_ruby_integ_tests() -> None:
    """Run commands for the Ruby integration tests directory."""
    repo_root = get_repo_root()
    base_cmd = f"""
      cd {repo_root}/integ-tests/ruby
      rake compile
      rake generate
      rake test
    """
    subprocess.run(base_cmd, shell=True, check=True)