"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.cliVersion = exports.cliBuild = void 0;
var child_process_1 = require("child_process");
function cliBuild(cliPath, workspacePath, onError, onSuccess) {
    var buildCommand = "".concat(cliPath, " build");
    if (!workspacePath) {
        return;
    }
    (0, child_process_1.exec)(buildCommand, {
        cwd: workspacePath.fsPath,
    }, function (error, stdout, stderr) {
        if (stdout) {
            console.log(stdout);
            // outputChannel.appendLine(stdout);
        }
        if (stderr) {
            // our CLI is by default logging everything to stderr
            console.info(stderr);
        }
        if (error) {
            console.error("Error running the build script: ".concat(JSON.stringify(error, null, 2)));
            onError === null || onError === void 0 ? void 0 : onError("Baml build error");
            return;
        }
        else {
            if (onSuccess) {
                onSuccess();
            }
        }
    });
}
exports.cliBuild = cliBuild;
function cliVersion(cliPath, onError, onSuccess) {
    var buildCommand = "".concat(cliPath, " --version");
    (0, child_process_1.exec)(buildCommand, function (error, stdout, stderr) {
        if (stderr) {
            // our CLI is by default logging everything to stderr
            console.info(stderr);
        }
        if (error) {
            onError === null || onError === void 0 ? void 0 : onError("Baml cli error");
            return;
        }
        else {
            if (onSuccess) {
                onSuccess(stdout);
            }
        }
    });
}
exports.cliVersion = cliVersion;
