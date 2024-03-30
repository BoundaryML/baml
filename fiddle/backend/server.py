from datetime import datetime as dt
from flask import Flask, jsonify, request
import flask_cors
import os
import subprocess as sp
import pprint

app = Flask(__name__)
flask_cors.CORS(app)

fiddle_dir = os.environ.get("FIDDLE_DIR", "/tmp/sam-fiddle")

@app.route("/")
def hello_world():
    return "<p>Hello, World!</p>"

#@flask_cors.cross_origin(["http://localhost:8000/", "http://localhost:3000/"]) 
@app.route("/fiddle", methods = ['POST'])
def fiddle():
    if os.path.exists(fiddle_dir):
        os.rename(fiddle_dir, f"{fiddle_dir}_{dt.now().strftime('%Y-%m-%d_%H-%M-%S')}")
    os.makedirs(fiddle_dir, exist_ok=True)

    for fname, contents in request.json['files'].items():
        fname = os.path.join(fiddle_dir, "baml_src", fname)
        fparent = os.path.dirname(fname)

        if fparent != fiddle_dir:
            os.makedirs(fparent, exist_ok=True)

        with open(fname, 'w') as file:
            file.write(contents)

    build_cmd = sp.run(
        "baml build".split(),
        cwd=fiddle_dir,
        capture_output=True,
    )
    test_cmd = sp.run(
        "baml test run".split(),
        cwd=fiddle_dir,
        capture_output=True,
    )

    results = {
        'build': {
            'returncode': build_cmd.returncode,
            'stdout': repr(build_cmd.stdout),
            'stderr': repr(build_cmd.stderr),
        },
        'test': {
            'returncode': test_cmd.returncode,
            'stdout': repr(test_cmd.stdout),
            'stderr': repr(test_cmd.stderr),
        }
    }

    print("Result is")
    pprint.pprint(results)

    return jsonify(results)

if __name__ == '__main__':
    os.makedirs("/tmp/baml", exist_ok=True)
    app.run(host="0.0.0.0", port=8000)