#!/usr/bin/env -S uv run --script

# /// script
# dependencies = [
#   "bump2version",
#   "rich",
#   "termcolor",
#   "typer",
# ]
# ///

import os
import subprocess as sp
import sys
from typing import Optional, Literal
import typer
from rich.console import Console

# Custom types
VersionBumpType = Literal["minor", "patch"]
CommandOutput = sp.CompletedProcess[str]

app = typer.Typer(add_completion=False)

c = Console()


def run(
    cmd: str, check: bool = True, capture_output: bool = False, text: bool = True
) -> CommandOutput:
    """Wrapper around subprocess.run that accepts shell-style commands"""
    result = sp.run(
        cmd, shell=True, check=check, capture_output=capture_output, text=text
    )
    return result


def ensure_preconditions() -> None:
    # Check for git-cliff
    try:
        run("git cliff -h", capture_output=True)
    except sp.CalledProcessError:
        c.print(
            "Error: git-cliff is not installed or not working. Please install it using 'cargo install git-cliff'.",
            style="red",
        )
        sys.exit(1)


def bump2version(*args: str) -> None:
    cmd = f"bump2version {' '.join(args)}"
    run(cmd)


def get_repo_root() -> str:
    return run("git rev-parse --show-toplevel", capture_output=True).stdout.strip()


def get_current_version() -> Optional[str]:
    with open("tools/versions/engine.cfg", "r") as f:
        for line in f:
            if "current_version =" in line:
                return line.split()[2]
    return None


def check_git_changes(pre_bump_version: str) -> int:
    try:
        diff_output = run(
            f"git diff {pre_bump_version} -- engine/language_client_codegen/src",
            capture_output=True,
        ).stdout
    except sp.CalledProcessError:
        diff_output = ""

    if not diff_output:
        c.print("No changes detected.")
        return 0

    changes = diff_output.count("diff --git")
    c.print(f"Detected changes: {changes}")
    return changes


@app.command()
def main(
    ts: bool = typer.Option(False, "--ts", help="Bump patch for typescript"),
    python: bool = typer.Option(False, "--python", help="Bump patch for python"),
    ruby: bool = typer.Option(False, "--ruby", help="Bump patch for ruby"),
    vscode: bool = typer.Option(False, "--vscode", help="Bump patch for vscode"),
    bump_all: bool = typer.Option(False, "--all", help="Bump all versions"),
    allow_dirty: bool = typer.Option(
        False, "--allow-dirty", help="Allow dirty git status"
    ),
) -> None:
    # Replace VersionBumpArgs with direct flag access
    modes = [ts, python, ruby, vscode, bump_all]
    if sum(modes) > 1:
        c.print("Error: Only one mode can be enabled.", style="red")
        sys.exit(1)
    elif sum(modes) == 0:
        bump_all = True

    ensure_preconditions()

    repo_root = get_repo_root()
    os.chdir(repo_root)

    # Pull latest tags
    run("git pull --tags")

    # Check git status
    if run("git diff --quiet", check=False).returncode != 0:
        if not allow_dirty:
            c.print(
                "Error: Git status is not clean. Please commit or stash your changes. (To bypass this, use --allow-dirty)",
                style="red",
            )
            sys.exit(1)
        c.print(
            "Warning: Git status is not clean. Proceeding due to --allow-dirty flag.",
            style="yellow",
        )

    pre_bump_version = get_current_version()
    if pre_bump_version is None:
        c.print("Error: Could not determine current version.", style="red")
        sys.exit(1)

    c.print(
        f"Checking for changes from version {pre_bump_version} in 'engine/language_client_codegen/src'...",
        style="blue",
    )

    code_gen_changes = check_git_changes(pre_bump_version)
    c.print(
        f"Number of code generation changes since {pre_bump_version}: {code_gen_changes}",
        style="blue",
    )

    suggested_version_bump: VersionBumpType = (
        "minor" if code_gen_changes > 0 else "patch"
    )
    if code_gen_changes > 0:
        c.print(
            f"Code generation changes detected since {pre_bump_version}. Recommending a [yellow]minor[/yellow] version bump."
        )
    else:
        c.print("no code change")
        c.print(
            f"No code generation changes detected since {pre_bump_version}. Recommending a [green]patch[/green] version bump."
        )

    selected_version_bump = typer.prompt(
        "Please confirm the version bump type (minor/patch)",
        default=suggested_version_bump,
        type=VersionBumpType,
    )
    if selected_version_bump not in ("minor", "patch"):
        print("Error: Invalid version bump type.")
        sys.exit(1)
    if selected_version_bump != suggested_version_bump:
        c.print(
            f"Warning: You selected {selected_version_bump} instead of the recommended {suggested_version_bump}.",
            style="yellow",
        )
        confirm = typer.confirm(
            "Are you sure you want to proceed with this version bump type?",
            default=False,
        )
        if not confirm:
            print("Aborting version bump.")
            sys.exit(1)

    perform_version_bumps(
        ts,
        python,
        ruby,
        vscode,
        bump_all,
        allow_dirty,
        selected_version_bump,
        repo_root,
    )

    os.chdir(repo_root)
    new_version = get_current_version()
    if new_version is None:
        print("Error: Could not determine new version.")
        sys.exit(1)

    branch_name = f"bump-version-{new_version}"

    # Handle existing branch
    if (
        run(
            f"git show-ref --verify --quiet refs/heads/{branch_name}", check=False
        ).returncode
        == 0
    ):
        delete_confirmation = typer.prompt(
            f"Branch {branch_name} already exists. Do you want to delete it?",
            type=str,
            default="no",
            show_default=False,
        )
        if delete_confirmation.lower() != "yes":
            print("Exiting without creating a new branch.")
            sys.exit(1)
        run(f"git branch -D {branch_name}")

    # Create and switch to new branch
    run(f"git checkout -b {branch_name}")
    run("git add .")
    run(f'git commit -m "Bump version to {new_version}"')

    handle_changelog(repo_root, new_version)

    run("git add CHANGELOG.md fern/pages/changelog.mdx")
    run("git commit --amend --no-edit")

    # Run build scripts
    os.chdir(os.path.join(repo_root, "integ-tests/typescript"))
    run("pnpm build")
    run("pnpm generate")

    os.chdir(os.path.join(repo_root, "integ-tests/python"))
    run("uv sync")
    run(
        f"uv run maturin develop --uv --manifest-path {repo_root}/engine/language_client_python/Cargo.toml"
    )
    run("uv run baml-cli generate --from ../baml_src")

    # Run integration tests
    ts_tests_status, python_tests_status = run_integration_tests(repo_root)

    os.chdir(repo_root)
    run("git add .")
    run(f'git commit -m "Run integ tests for {new_version}"')

    print(f"All done! Please push the branch {branch_name} and create a PR.")

    commit_body = f"""Bump version to {new_version}

{ts_tests_status}
{python_tests_status}

Generated by bump-version script."""

    run(
        f'''gh pr create --title "chore: Bump version to {new_version}" --body "{commit_body}"'''
    )


def handle_changelog(repo_root: str, new_version: str) -> None:
    run(f"git cliff --tag {new_version} -u --prepend CHANGELOG.md")
    print(f"Version bumped to {new_version} successfully! Please update CHANGELOG.md.")

    while True:
        changelog_confirmation = input(
            "Have you updated CHANGELOG.md? Type 'yes' to confirm: "
        )
        if changelog_confirmation == "yes":
            with open("CHANGELOG.md", "r") as f:
                if "### UNMATCHED" in f.read():
                    print(
                        "### UNMATCHED section found in CHANGELOG.md. Please remove it before proceeding."
                    )
                    continue
            break
        print("Please update CHANGELOG.md before proceeding.")

    run("cp CHANGELOG.md fern/pages/changelog.mdx")
    with open("fern/pages/changelog.mdx", "r") as f:
        content = f.read()
    with open("fern/pages/changelog.mdx", "w") as f:
        f.write("---\ntitle: Changelog\n---\n" + content[content.find("\n") + 1 :])


def run_integration_tests(repo_root: str) -> tuple[str, str]:
    os.chdir(os.path.join(repo_root, "integ-tests/typescript"))
    ts_tests_status = "✅ Typescript integ tests"
    try:
        run("pnpm integ-tests:ci")
    except sp.CalledProcessError:
        ts_tests_status = "❌ Typescript integ tests"
        print("Typescript integ tests failed, but continuing...")

    os.chdir(os.path.join(repo_root, "integ-tests/python"))
    python_tests_status = "✅ Python integ tests"
    try:
        run("infisical run --env=test -- uv run pytest")
    except sp.CalledProcessError:
        python_tests_status = "❌ Python integ tests"
        print("Python integ tests failed, but continuing...")

    return ts_tests_status, python_tests_status


def perform_version_bumps(
    ts: bool,
    python: bool,
    ruby: bool,
    vscode: bool,
    all: bool,
    allow_dirty: bool,
    user_confirmation: VersionBumpType,
    repo_root: str,
) -> None:
    os.chdir(os.path.join(repo_root, "tools"))

    if all:
        bump2version(
            "--config-file",
            "./versions/engine.cfg",
            user_confirmation,
            "--allow-dirty" if allow_dirty else "",
        )
        for config in ["python", "typescript", "ruby", "vscode", "integ-tests"]:
            bump2version(
                "--config-file",
                f"./versions/{config}.cfg",
                user_confirmation,
                "--allow-dirty" if allow_dirty else "",
            )
    elif ts:
        bump2version("--config-file", "./versions/typescript.cfg", "patch")
    elif python:
        bump2version("--config-file", "./versions/python.cfg", "patch")
    elif ruby:
        bump2version("--config-file", "./versions/ruby.cfg", "patch")
    elif vscode:
        bump2version("--config-file", "./versions/vscode.cfg", "patch")


if __name__ == "__main__":
    app()
