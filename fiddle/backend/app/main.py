import os
import shutil
import tempfile
from fastapi import FastAPI, Request, HTTPException, Depends, BackgroundTasks
from fastapi.responses import StreamingResponse
import subprocess
import asyncio
from pydantic import BaseModel
from typing import List, Optional, Dict
from dotenv import load_dotenv
from uuid import uuid4
from fastapi.middleware.cors import CORSMiddleware
from baml_client import baml
from baml_client.baml_types import LinterOutput


origins = [
    "http://localhost.tiangolo.com",
    "https://localhost.tiangolo.com",
    "http://localhost",
    "http://localhost:3000",
]

load_dotenv()


# class TestImplementation(BaseModel):
#     name: str


class Test(BaseModel):
    name: str
    impls: List[str]


class Function(BaseModel):
    name: str
    tests: List[Test]
    run_all_available_tests: Optional[bool] = False


class TestRequest(BaseModel):
    functions: List[Function]


class FileModel(BaseModel):
    name: str
    content: str


class RunTests(BaseModel):
    files: List[FileModel]
    testRequest: TestRequest


app = FastAPI()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

fiddle_dir = os.environ.get("FIDDLE_DIR", "/tmp/fiddle")

generator_block = """\
generator lang_python {
  language python
  // This is where your non-baml source code located
  // (relative directory where pyproject.toml, package.json, etc. lives)
  project_root ".."
  // This command is used by "baml test" to run tests
  // defined in the playground
  test_command "pytest -s"
  // This command is used by "baml update-client" to install
  // dependencies to your language environment
  install_command "poetry add baml@latest"
  package_version_command "poetry show baml"
}
"""


async def process_output(process):
    async for line in process.stdout:
        yield "STDOUT", line.decode()
    async for line in process.stderr:
        yield "STDERR", line.decode()


async def handle_client(reader, writer, output_queue: asyncio.Queue):
    while True:
        data = await reader.readline()
        if not data:
            break
        message = data.decode()
        await output_queue.put(("PORT", message))


async def generator(output_queue: asyncio.Queue):
    while True:
        output = await output_queue.get()
        if output is None:  # None is the signal to stop
            break
        source, line = output
        yield f"data: <BAML_{source}>: {line}\n\n"


async def stream_subprocess_and_port_output(command, cwd, output_queue: asyncio.Queue):
    # Initialize the server to listen on an available port
    server = await asyncio.start_server(
        lambda r, w: handle_client(r, w, output_queue), "0.0.0.0", 0
    )
    port = server.sockets[0].getsockname()[1]
    # Serve until the subprocess exits
    output_task = None
    try:
        # Adjust the command to include the dynamically determined port
        full_command = command.format(port=port)
        print("running command: ", full_command)
        process = await asyncio.create_subprocess_shell(
            full_command,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=cwd,
            shell=True,
            env=os.environ.copy(),
        )

        # Function to forward subprocess output to the queue
        async def forward_output():
            async for source, line in process_output(process):
                await output_queue.put((source, line))

        # Run the forward_output task alongside the server
        output_task = asyncio.create_task(forward_output())

        await process.wait()
        print("Process exited---------")
    finally:
        print("Closing server---------")
        # Attempt to clean up the directory
        try:
            shutil.rmtree(cwd)
        except Exception as e:
            print(f"Error removing directory {cwd}: {e}")

        server.close()
        await server.wait_closed()
        print("Cancelling task---------")
        if output_task is not None:
            output_task.cancel()
        # Indicate completion by putting None into the queue
        await output_queue.put(None)


async def create_temp_files():
    dir_to_use = f"{fiddle_dir}-{uuid4()}"
    os.makedirs(f"{dir_to_use}", exist_ok=True)
    try:
        yield dir_to_use
    finally:
        pass
        # shutil.rmtree(fiddle_dir)


@app.get("/")
def hello_world():
    return "<p>Hello, World!</p>"


@app.post("/fiddle")
async def fiddle(request: RunTests, tmpdir: str = Depends(create_temp_files)):
    files = request.files
    test_request = request.testRequest
    print(request)

    for file in files:
        if "main.baml" in file.name:
            file.content = generator_block + file.content
        # Ensure the directory path exists
        file_directory = os.path.join(tmpdir, os.path.dirname(file.name))
        os.makedirs(file_directory, exist_ok=True)  # Create any directories in the path

        # Now it's safe to write the file
        file_path = os.path.join(tmpdir, file.name)
        with open(file_path, "w") as f:
            f.write(file.content)

    await asyncio.sleep(1.0)
    # Use asyncio subprocess for non-blocking call
    process = await asyncio.create_subprocess_shell(
        "baml build",
        cwd=tmpdir,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
    )
    stdout, stderr = await process.communicate()
    print("--------- baml build output ---------")
    print(stdout.decode())
    print(stderr.decode())
    if process.returncode != 0:
        raise HTTPException(status_code=400, detail="BAML build failed")

    output_queue = asyncio.Queue()
    if test_request:
        selected_tests = [
            f"-i {fn.name}:{impl}:{test.name}"
            for fn in test_request.functions
            for test in fn.tests
            for impl in test.impls
        ]

        is_single_function = len(test_request.functions) == 1
        test_filter = (
            f"-i {test_request.functions[0].name}:"
            if is_single_function and test_request.functions[0].run_all_available_tests
            else " ".join(selected_tests)
        )
    else:
        test_filter = ""

    # Modify the test_command to include test_filter
    test_command = f"baml test {test_filter} run --playground-port {{port}}"

    streaming_gen = generator(output_queue)
    asyncio.create_task(
        stream_subprocess_and_port_output(test_command, tmpdir, output_queue)
    )

    # Corrected to await the streaming function correctly
    return StreamingResponse(streaming_gen, media_type="text/plain")


class LintRequest(BaseModel):
    lintingRules: List[str]
    promptTemplate: str
    promptVariables: Dict[str, str]


class LinterRuleOutput(BaseModel):
    diagnostics: List[LinterOutput]
    ruleName: str


@app.post("/lint")
async def lint(request: LintRequest) -> List[LinterRuleOutput]:
    result1, result2, res3, res4, res5 = await asyncio.gather(
        baml.Contradictions(request.promptTemplate),
        baml.ChainOfThought(request.promptTemplate),
        baml.AmbiguousTerm(request.promptTemplate),
        baml.OffensiveLanguage(request.promptTemplate),
        baml.ExampleProvider(request.promptTemplate),
    )

    res1_outputs = [
        LinterOutput(
            exactPhrase=item.exactPhrase,
            recommendation=item.recommendation,
            fix=item.fix,
            reason=item.reason,
        )
        for item in result1
    ]

    print(result1)
    print(result2)
    print(res3)

    return [
        LinterRuleOutput(
            diagnostics=res1_outputs,
            ruleName="Contradictions",
        ),
        LinterRuleOutput(diagnostics=result2, ruleName="ChainOfThought"),
        LinterRuleOutput(diagnostics=res3, ruleName="AmbiguousTerm"),
        LinterRuleOutput(diagnostics=res4, ruleName="OffensiveLanguage"),
        LinterRuleOutput(diagnostics=res5, ruleName="ExampleProvider"),
    ]


# if __name__ == '__main__':
# os.makedirs("/tmp/baml", exist_ok=True)
# app.run(host="0.0.0.0", port=8000)
