import os
import shutil
import tempfile
from fastapi import FastAPI, Request, HTTPException, Depends
from fastapi.responses import StreamingResponse
import subprocess
import asyncio
from pydantic import BaseModel
from typing import List
from dotenv import load_dotenv

load_dotenv()

class FileModel(BaseModel):
    name: str
    content: str

class FiddleRequest(BaseModel):
    files: List[FileModel]


app = FastAPI()

fiddle_dir = os.environ.get("FIDDLE_DIR", "/tmp/fiddle")


async def stream_subprocess_output(command, cwd):
    process = await asyncio.create_subprocess_shell(
        command, stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.PIPE, cwd=cwd
    )
    
    async def stream_output(stream):
        while True:
            line = await stream.readline()
            if not line:
                break
            yield line.decode('utf-8')
    
    async def combined_output():
        async for line in stream_output(process.stdout):
            yield line
        async for line in stream_output(process.stderr):
            yield line
    
    async for output in combined_output():
        yield output + "\n"

async def create_temp_files():
    dir = tempfile.TemporaryDirectory()
    try:
        yield dir.name
    finally:
        del dir

@app.get("/")
def hello_world():
    return "<p>Hello, World!</p>"


@app.post("/fiddle")
async def fiddle(request: FiddleRequest, tmpdir: str = Depends(create_temp_files)):
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

    if not os.path.exists(tmpdir):
        print(f"Directory {tmpdir} does not exist.")
    else:
        print(f"Directory {tmpdir} exists.")

    stdout, stderr = await process.communicate()
    print("--------- baml build output ---------")
    print(stdout.decode())
    print(stderr.decode())
    if process.returncode != 0:
        raise HTTPException(status_code=400, detail="BAML build failed")
    
    
    # Corrected to await the streaming function correctly
    return StreamingResponse(
        stream_subprocess_output("baml test run", cwd=tmpdir),
        media_type="text/plain",
        headers={"Content-Disposition": "attachment; filename=baml_test_output.txt"}
    )

#if __name__ == '__main__':
    # os.makedirs("/tmp/baml", exist_ok=True)
    # app.run(host="0.0.0.0", port=8000)