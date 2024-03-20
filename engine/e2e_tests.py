"""
File for testing baml

Run with:
infisical run --env=test -- python3 -m pytest e2e_tests.py -s -v
"""

import subprocess
import os
import pytest

CWD = os.path.dirname(os.path.realpath(__file__))


@pytest.fixture(scope="session", autouse=True)
def setup_baml_builder():
    try:
        subprocess.check_output(
            "docker build -f .docker/Dockerfile.builder -t baml_builder . -q",
            shell=True,
            stderr=subprocess.STDOUT,
            cwd=CWD,
        )
    except subprocess.CalledProcessError as e:
        print("Docker build for baml_builder failed.")
        print(e.output.decode("utf-8"))
        pytest.exit("Setup failed, exiting tests.", 1)


@pytest.fixture(scope="session")
def openai_key():
    return os.environ.get("OPENAI_API_KEY", "sk-unset-openai-key")


def get_test_cases():
    test_groups = []
    for test_dir in os.listdir(os.path.join(CWD, ".docker")):
        test_dir_path = os.path.join(CWD, ".docker", test_dir)
        if os.path.isdir(test_dir_path) and test_dir_path.endswith("-tests"):
            for tests in os.listdir(test_dir_path):
                if not os.path.isdir(os.path.join(test_dir_path, tests)):
                    continue
                test_groups.append((test_dir, tests))
    return test_groups


@pytest.mark.parametrize("context,tag", get_test_cases())
def test_docker_builds_and_runs(context: str, tag: str, openai_key: str):
    build_cmd: str = "docker build -q"
    subprocess.check_output(
        f"{build_cmd} --cache-from baml_builder -t baml_{tag} .docker/{context}/{tag}",
        shell=True,
        stderr=subprocess.STDOUT,
        cwd=CWD,
    )
    os.makedirs(f"{os.path.join(CWD, 'test_logs', tag)}", exist_ok=True)
    env_vars = {
        "OPENAI_API_KEY": openai_key,
    }
    env_vars_str = " ".join([f"-e {k}={v}" for k, v in env_vars.items()])

    run_cmd = f"docker run {env_vars_str} -v {os.path.join(CWD, 'test_logs', tag)}:/usr/src/logs baml_{tag}"
    subprocess.check_call(
        run_cmd,
        shell=True,
        cwd=CWD,
    )
