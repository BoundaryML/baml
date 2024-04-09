import os
import shutil
import tempfile
from fastapi import FastAPI, Request, HTTPException, Depends, BackgroundTasks
from fastapi.responses import StreamingResponse
import subprocess
import asyncio
from pydantic import BaseModel
from typing import List
from dotenv import load_dotenv
from uuid import uuid4
from fastapi.middleware.cors import CORSMiddleware

origins = [
    "http://localhost.tiangolo.com",
    "https://localhost.tiangolo.com",
    "http://localhost",
    "http://localhost:3000",
]

load_dotenv()

class FileModel(BaseModel):
    name: str
    content: str

class FiddleRequest(BaseModel):
    files: List[FileModel]


app = FastAPI()

app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

fiddle_dir = os.environ.get("FIDDLE_DIR", "/tmp/fiddle")

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
        yield f"data: {source}: {line}\n\n"

async def stream_subprocess_and_port_output(command, cwd, output_queue: asyncio.Queue):
    # Initialize the server to listen on an available port
    server = await asyncio.start_server(
        lambda r, w: handle_client(r, w, output_queue), 
        '0.0.0.0', 0
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
            env=os.environ.copy()
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
    dir_to_use = f'{fiddle_dir}-{uuid4()}'
    os.makedirs(f'{dir_to_use}', exist_ok=True)
    try:
        yield dir_to_use
    finally:
        pass
        #shutil.rmtree(fiddle_dir)

@app.get("/")
def hello_world():
    return "<p>Hello, World!</p>"


@app.post("/fiddle")
async def fiddle(request: FiddleRequest, tmpdir: str = Depends(create_temp_files) ):
    files = request.files
    print(files)    
    
    for file in files:
        # Ensure the directory path exists
        file_directory = os.path.join(tmpdir, os.path.dirname(file.name))
        os.makedirs(file_directory, exist_ok=True)  # Create any directories in the path

        # Now it's safe to write the file
        file_path = os.path.join(tmpdir, file.name)
        with open(file_path, "w") as f:
            f.write(file.content)

    # Use asyncio subprocess for non-blocking call
    process = await asyncio.create_subprocess_shell(
        "baml build", cwd=tmpdir, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE
    )
    stdout, stderr = await process.communicate()
    print("--------- baml build output ---------")
    print(stdout.decode())
    print(stderr.decode())
    if process.returncode != 0:
        raise HTTPException(status_code=400, detail="BAML build failed")
    
    output_queue = asyncio.Queue()
    test_command = "baml test run --playground-port {port}"
    streaming_gen = generator(output_queue)
    asyncio.create_task(stream_subprocess_and_port_output(test_command, tmpdir, output_queue))
   
    
    # Corrected to await the streaming function correctly
    return StreamingResponse(streaming_gen, media_type="text/plain")

#if __name__ == '__main__':
    # os.makedirs("/tmp/baml", exist_ok=True)
    # app.run(host="0.0.0.0", port=8000)