"use strict";
var __assign = (this && this.__assign) || function () {
    __assign = Object.assign || function(t) {
        for (var s, i = 1, n = arguments.length; i < n; i++) {
            s = arguments[i];
            for (var p in s) if (Object.prototype.hasOwnProperty.call(s, p))
                t[p] = s[p];
        }
        return t;
    };
    return __assign.apply(this, arguments);
};
var __awaiter = (this && this.__awaiter) || function (thisArg, _arguments, P, generator) {
    function adopt(value) { return value instanceof P ? value : new P(function (resolve) { resolve(value); }); }
    return new (P || (P = Promise))(function (resolve, reject) {
        function fulfilled(value) { try { step(generator.next(value)); } catch (e) { reject(e); } }
        function rejected(value) { try { step(generator["throw"](value)); } catch (e) { reject(e); } }
        function step(result) { result.done ? resolve(result.value) : adopt(result.value).then(fulfilled, rejected); }
        step((generator = generator.apply(thisArg, _arguments || [])).next());
    });
};
var __generator = (this && this.__generator) || function (thisArg, body) {
    var _ = { label: 0, sent: function() { if (t[0] & 1) throw t[1]; return t[1]; }, trys: [], ops: [] }, f, y, t, g;
    return g = { next: verb(0), "throw": verb(1), "return": verb(2) }, typeof Symbol === "function" && (g[Symbol.iterator] = function() { return this; }), g;
    function verb(n) { return function (v) { return step([n, v]); }; }
    function step(op) {
        if (f) throw new TypeError("Generator is already executing.");
        while (g && (g = 0, op[0] && (_ = 0)), _) try {
            if (f = 1, y && (t = op[0] & 2 ? y["return"] : op[0] ? y["throw"] || ((t = y["return"]) && t.call(y), 0) : y.next) && !(t = t.call(y, op[1])).done) return t;
            if (y = 0, t) op = [op[0] & 2, t.value];
            switch (op[0]) {
                case 0: case 1: t = op; break;
                case 4: _.label++; return { value: op[1], done: false };
                case 5: _.label++; y = op[1]; op = [0]; continue;
                case 7: op = _.ops.pop(); _.trys.pop(); continue;
                default:
                    if (!(t = _.trys, t = t.length > 0 && t[t.length - 1]) && (op[0] === 6 || op[0] === 2)) { _ = 0; continue; }
                    if (op[0] === 3 && (!t || (op[1] > t[0] && op[1] < t[3]))) { _.label = op[1]; break; }
                    if (op[0] === 6 && _.label < t[1]) { _.label = t[1]; t = op; break; }
                    if (t && _.label < t[2]) { _.label = t[2]; _.ops.push(op); break; }
                    if (t[2]) _.ops.pop();
                    _.trys.pop(); continue;
            }
            op = body.call(thisArg, _);
        } catch (e) { op = [6, e]; y = 0; } finally { f = t = 0; }
        if (op[0] & 5) throw op[1]; return { value: op[0] ? op[1] : void 0, done: true };
    }
};
Object.defineProperty(exports, "__esModule", { value: true });
var vscode = require("vscode");
var net_1 = require("net");
var path = require("path");
var child_process_1 = require("child_process");
var common_1 = require("@baml/common");
var util_1 = require("../util");
var outputChannel = vscode.window.createOutputChannel('baml-test-runner');
function __initServer(messageHandler) {
    var server = net_1.default.createServer(function (socket) {
        console.log('Python script connected');
        socket.on('data', messageHandler);
        socket.on('end', function () {
            console.log('Python script disconnected');
        });
    });
    server.listen(0, '127.0.0.1');
    return server;
}
var TestState = /** @class */ (function () {
    function TestState() {
        this.testStateListener = undefined;
        this.handleMessage = this.handleMessage.bind(this);
        this.handleLog = this.handleLog.bind(this);
        this.test_results = {
            results: [],
            test_url: null,
            run_status: 'NOT_STARTED',
        };
    }
    TestState.prototype.setTestStateListener = function (listener) {
        this.testStateListener = listener;
    };
    TestState.prototype.clearTestCases = function () {
        var _a;
        this.test_results = {
            results: [],
            test_url: null,
            run_status: 'NOT_STARTED',
        };
        (_a = this.testStateListener) === null || _a === void 0 ? void 0 : _a.call(this, this.test_results);
    };
    TestState.prototype.initializeTestCases = function (tests) {
        var _a;
        this.test_results = {
            results: tests.functions.flatMap(function (fn) {
                return fn.tests.flatMap(function (test) {
                    return test.impls.map(function (impl) { return ({
                        fullTestName: (0, common_1.getFullTestName)(test.name, impl, fn.name),
                        functionName: fn.name,
                        testName: test.name,
                        implName: impl,
                        status: common_1.TestStatus.Compiling,
                        output: {},
                    }); });
                });
            }),
            run_status: 'RUNNING',
            exit_code: undefined,
            test_url: null,
        };
        (_a = this.testStateListener) === null || _a === void 0 ? void 0 : _a.call(this, this.test_results);
    };
    TestState.prototype.handleMessage = function (data) {
        var _this = this;
        try {
            // Data may be inadvertently concatenated together, but we actually send a \n delimiter between messages to be able to split msgs properly.
            var delimitedData = data.toString().split('<END_MSG>\n');
            delimitedData.forEach(function (data) {
                if (data) {
                    _this.handleMessageLine(data);
                }
            });
        }
        catch (e) {
            console.error(e);
            outputChannel.appendLine(JSON.stringify(e, null, 2));
        }
    };
    TestState.prototype.handleMessageLine = function (data) {
        var payload = JSON.parse(data.toString());
        switch (payload.name) {
            case 'test_url':
                this.setTestUrl(payload.data);
                break;
            case 'update_test_case':
                this.handleUpdateTestCase(payload.data);
                break;
            case 'log':
                var res = common_1.clientEventLogSchema.safeParse(payload.data);
                if (!res.success) {
                    // console.error(res.error)
                }
                else {
                    this.handleLog(payload.data);
                }
                break;
        }
    };
    TestState.prototype.setExitCode = function (code) {
        var _a;
        this.test_results.exit_code = code;
        if (code === undefined) {
            this.test_results.run_status = 'NOT_STARTED';
        }
        else if (code === 0) {
            this.test_results.run_status = 'COMPLETED';
        }
        else {
            this.test_results.run_status = 'ERROR';
        }
        (_a = this.testStateListener) === null || _a === void 0 ? void 0 : _a.call(this, this.test_results);
    };
    TestState.prototype.getTestResults = function () {
        return this.test_results;
    };
    TestState.prototype.setTestUrl = function (testUrl) {
        var _a;
        this.test_results.test_url = testUrl.dashboard_url;
        this.test_results.results.forEach(function (test) {
            test.status = common_1.TestStatus.Queued;
        });
        (_a = this.testStateListener) === null || _a === void 0 ? void 0 : _a.call(this, this.test_results);
    };
    TestState.prototype.handleUpdateTestCase = function (data) {
        var _a;
        var testResult = this.test_results.results.find(function (test) { return test.fullTestName === data.test_case_arg_name; });
        if (testResult) {
            testResult.status = data.status;
            if (data.error_data) {
                testResult.output = {
                    error: JSON.stringify(data.error_data),
                };
            }
            (_a = this.testStateListener) === null || _a === void 0 ? void 0 : _a.call(this, this.test_results);
        }
    };
    TestState.prototype.handleLog = function (data) {
        var _a, _b, _c, _d, _e, _f, _g, _h, _j;
        var fullTestName = (_a = data.context.tags) === null || _a === void 0 ? void 0 : _a['test_case_arg_name'];
        var testResult = this.test_results.results.find(function (test) { return test.fullTestName === fullTestName; });
        if (testResult && data.event_type === 'func_llm') {
            if (this.test_results.test_url) {
                testResult.url = "".concat(this.test_results.test_url, "&s_eid=").concat(data.event_id, "&eid=").concat(data.root_event_id);
            }
            testResult.output = {
                error: (_c = (_b = data.error) === null || _b === void 0 ? void 0 : _b.message) !== null && _c !== void 0 ? _c : testResult.output.error,
                parsed: (_e = (_d = data.io.output) === null || _d === void 0 ? void 0 : _d.value) !== null && _e !== void 0 ? _e : testResult.output.parsed,
                raw: (_h = (_g = (_f = data.metadata) === null || _f === void 0 ? void 0 : _f.output) === null || _g === void 0 ? void 0 : _g.raw_text) !== null && _h !== void 0 ? _h : testResult.output.raw,
            };
            (_j = this.testStateListener) === null || _j === void 0 ? void 0 : _j.call(this, this.test_results);
        }
    };
    return TestState;
}());
var TestExecutor = /** @class */ (function () {
    function TestExecutor() {
        this.stdoutListener = undefined;
        this.currentProcess = undefined;
        this.server = undefined;
        this.testState = new TestState();
    }
    TestExecutor.prototype.getTestResults = function () {
        return this.testState.getTestResults();
    };
    TestExecutor.prototype.setTestStateListener = function (listener) {
        this.testState.setTestStateListener(listener);
    };
    TestExecutor.prototype.setStdoutListener = function (listener) {
        this.stdoutListener = listener;
    };
    TestExecutor.prototype.start = function () {
        if (this.server !== undefined) {
            return;
        }
        this.server = __initServer(this.testState.handleMessage);
    };
    Object.defineProperty(TestExecutor.prototype, "port_arg", {
        get: function () {
            if (this.server !== undefined) {
                var addr = this.server.address();
                // vscode.window.showInformationMessage(`Server address: ${JSON.stringify(addr)}`)
                if (typeof addr === 'string') {
                    return "--playground-port ".concat(addr);
                }
                else if (addr) {
                    return "--playground-port ".concat(addr.port);
                }
            }
            vscode.window.showErrorMessage('Server not initialized');
            return '';
        },
        enumerable: false,
        configurable: true
    });
    TestExecutor.prototype.getPythonPath = function () {
        return __awaiter(this, void 0, void 0, function () {
            var _a;
            return __generator(this, function (_b) {
                switch (_b.label) {
                    case 0:
                        if (!(TestExecutor.pythonPath === undefined)) return [3 /*break*/, 2];
                        // Check if we should use python3 by seeing if shell has python3
                        _a = TestExecutor;
                        return [4 /*yield*/, new Promise(function (resolve, reject) {
                                var _a;
                                var res = (0, child_process_1.exec)('python3 -s --version');
                                (_a = res.stdout) === null || _a === void 0 ? void 0 : _a.on('data', function (data) {
                                    console.log("stdout: ".concat(data));
                                });
                                res.on('exit', function (code, signal) {
                                    console.log("exit: ".concat(code));
                                    if (code === 0) {
                                        resolve('python3');
                                    }
                                    else {
                                        resolve('python');
                                    }
                                });
                            })];
                    case 1:
                        // Check if we should use python3 by seeing if shell has python3
                        _a.pythonPath = _b.sent();
                        _b.label = 2;
                    case 2:
                        console.log("Using python path: ".concat(TestExecutor.pythonPath));
                        return [2 /*return*/, TestExecutor.pythonPath];
                }
            });
        });
    };
    TestExecutor.prototype.runTest = function (_a) {
        var _b, _c, _d, _e, _f;
        var root_path = _a.root_path, tests = _a.tests;
        return __awaiter(this, void 0, void 0, function () {
            var selectedTests, is_single_function, test_filter, command, cp, e_1;
            var _this = this;
            return __generator(this, function (_g) {
                switch (_g.label) {
                    case 0:
                        _g.trys.push([0, 4, , 5]);
                        // root_path is the path to baml_src, so go up one level to get to the root of the project
                        console.log("Running tests in ".concat(root_path));
                        return [4 /*yield*/, this.cancelExistingTestRun()];
                    case 1:
                        _g.sent();
                        root_path = path.join(root_path, '../');
                        this.testState.initializeTestCases(tests);
                        return [4 /*yield*/, vscode.commands.executeCommand('workbench.action.files.saveAll')];
                    case 2:
                        _g.sent();
                        // There is a bug where there are still some .tmp files that make the compiler bug out on `baml build` if we don't wait
                        // a second before running the tests.
                        // This is likely because VSCode adds the save event to the NodeJS event loop, so
                        // we have to wait for the next tick to ensure the files are actually saved.
                        // Awaiting a promise is the easiest way to do this.
                        return [4 /*yield*/, new Promise(function (resolve, reject) {
                                setTimeout(function () {
                                    resolve(undefined);
                                }, 100);
                            })];
                    case 3:
                        // There is a bug where there are still some .tmp files that make the compiler bug out on `baml build` if we don't wait
                        // a second before running the tests.
                        // This is likely because VSCode adds the save event to the NodeJS event loop, so
                        // we have to wait for the next tick to ensure the files are actually saved.
                        // Awaiting a promise is the easiest way to do this.
                        _g.sent();
                        selectedTests = tests.functions.flatMap(function (fn) {
                            return fn.tests.flatMap(function (test) { return test.impls.map(function (impl) { return "-i ".concat(fn.name, ":").concat(impl, ":").concat(test.name); }); });
                        });
                        is_single_function = tests.functions.length === 1;
                        test_filter = is_single_function && ((_b = tests.functions[0]) === null || _b === void 0 ? void 0 : _b.run_all_available_tests) ? "-i '".concat(tests.functions[0].name, ":'") : selectedTests.join(' ');
                        command = "".concat((0, util_1.bamlPath)({ for_test: true }), " test ").concat(test_filter, " run ").concat(this.port_arg);
                        // Run the Python script in a child process
                        // const process = spawn(pythonExecutable, [tempFilePath]);
                        // Run the Python script using exec
                        (_c = this.stdoutListener) === null || _c === void 0 ? void 0 : _c.call(this, '<BAML_RESTART>');
                        (_d = this.stdoutListener) === null || _d === void 0 ? void 0 : _d.call(this, "\u001B[90mRunning BAML Test: ".concat(command, "\n\u001B[0m"));
                        cp = (0, child_process_1.exec)(command, {
                            cwd: root_path,
                            shell: (0, util_1.bamlTestShell)(),
                            env: __assign(__assign({}, process.env), { CLICOLOR_FORCE: '1' }),
                        });
                        this.currentProcess = cp;
                        (_e = cp.stdout) === null || _e === void 0 ? void 0 : _e.on('data', function (data) {
                            var _a;
                            outputChannel.appendLine(data);
                            (_a = _this.stdoutListener) === null || _a === void 0 ? void 0 : _a.call(_this, data);
                        });
                        (_f = cp.stderr) === null || _f === void 0 ? void 0 : _f.on('data', function (data) {
                            var _a;
                            outputChannel.appendLine(data);
                            (_a = _this.stdoutListener) === null || _a === void 0 ? void 0 : _a.call(_this, data);
                        });
                        cp.on('exit', function (code, signal) {
                            console.log("test exit code: ".concat(code, " signal: ").concat(signal));
                            // Dont mark it as an error if we killed it ourselves
                            _this.testState.setExitCode(code !== null && code !== void 0 ? code : (signal ? 0 : 5));
                            if (code === null && signal === 'SIGTERM') {
                                _this.testState.clearTestCases();
                            }
                            _this.currentProcess = undefined;
                        });
                        return [3 /*break*/, 5];
                    case 4:
                        e_1 = _g.sent();
                        console.error(e_1);
                        outputChannel.appendLine(JSON.stringify(e_1, null, 2));
                        this.testState.setExitCode(5);
                        this.currentProcess = undefined;
                        return [3 /*break*/, 5];
                    case 5: return [2 /*return*/];
                }
            });
        });
    };
    TestExecutor.prototype.cancelExistingTestRun = function () {
        var _a, _b;
        return __awaiter(this, void 0, void 0, function () {
            var res;
            var _this = this;
            return __generator(this, function (_c) {
                switch (_c.label) {
                    case 0:
                        this.testState.clearTestCases();
                        this.testState.setExitCode(undefined);
                        if (!this.currentProcess) {
                            return [2 /*return*/];
                        }
                        console.log("Killing existing process", (_a = this.currentProcess) === null || _a === void 0 ? void 0 : _a.pid);
                        res = this.currentProcess.kill();
                        if (!res) {
                            console.log("Failed to kill process", (_b = this.currentProcess) === null || _b === void 0 ? void 0 : _b.pid);
                            vscode.window.showErrorMessage('Failed to kill existing test process');
                        }
                        // do an interval and check for the current process to be undefined and await
                        // The var gets set to undefined in the .on('exit') handler
                        return [4 /*yield*/, new Promise(function (resolve, reject) {
                                var timeout = setTimeout(function () {
                                    clearInterval(interval);
                                    resolve(undefined);
                                }, 10000);
                                var interval = setInterval(function () {
                                    if (!_this.currentProcess) {
                                        clearTimeout(timeout);
                                        clearInterval(interval);
                                        resolve(undefined);
                                    }
                                }, 100);
                            })];
                    case 1:
                        // do an interval and check for the current process to be undefined and await
                        // The var gets set to undefined in the .on('exit') handler
                        _c.sent();
                        return [2 /*return*/];
                }
            });
        });
    };
    TestExecutor.prototype.close = function () {
        if (this.server) {
            this.server.close();
        }
    };
    TestExecutor.pythonPath = undefined;
    return TestExecutor;
}());
var testExecutor = new TestExecutor();
exports.default = testExecutor;
