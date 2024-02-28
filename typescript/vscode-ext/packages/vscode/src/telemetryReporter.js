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
var vscode_1 = require("vscode");
var hashes_1 = require("./hashes");
var posthog_node_1 = require("posthog-node");
var vscode = require("vscode");
var os_1 = require("os");
var client = new posthog_node_1.PostHog('phc_732PWG6HFZ75S7h0TK2AuqRVkqZDiD4WePE9gXYJkOu');
var TelemetryReporter = /** @class */ (function () {
    function TelemetryReporter(extensionId, extensionVersion) {
        var _this = this;
        this.extensionId = extensionId;
        this.extensionVersion = extensionVersion;
        this.userOptIn = false;
        this.telemetryProps = {};
        this.updateUserOptIn();
        this.configListener = vscode_1.workspace.onDidChangeConfiguration(function () { return _this.updateUserOptIn(); });
    }
    TelemetryReporter.prototype.initialize = function () {
        return __awaiter(this, void 0, void 0, function () {
            var machine_id, properties;
            var _a;
            return __generator(this, function (_b) {
                switch (_b.label) {
                    case 0:
                        if (!this.userOptIn) return [3 /*break*/, 2];
                        machine_id = vscode.env.machineId;
                        _a = {
                            extension: this.extensionId,
                            version: this.extensionVersion
                        };
                        return [4 /*yield*/, (0, hashes_1.getProjectHash)()];
                    case 1:
                        properties = (_a.project_hash = _b.sent(),
                            _a.machine_id = machine_id,
                            _a.session_id = vscode.env.sessionId,
                            _a.vscode_version = vscode.version,
                            _a.os_info = {
                                release: os_1.default.release(),
                                platform: os_1.default.platform(),
                                arch: os_1.default.arch(),
                            },
                            _a);
                        this.telemetryProps = properties;
                        client.capture({
                            event: "extension_loaded",
                            distinctId: machine_id,
                            properties: properties
                        });
                        client.flush();
                        _b.label = 2;
                    case 2: return [2 /*return*/];
                }
            });
        });
    };
    TelemetryReporter.prototype.sendTelemetryEvent = function (data) {
        return __awaiter(this, void 0, void 0, function () {
            return __generator(this, function (_a) {
                if (this.userOptIn) {
                    client.capture({
                        event: data.event,
                        distinctId: vscode.env.machineId,
                        properties: __assign(__assign({}, this.telemetryProps), data.properties)
                    });
                    client.flush();
                }
                return [2 /*return*/];
            });
        });
    };
    TelemetryReporter.prototype.updateUserOptIn = function () {
        var telemetrySettings = vscode_1.workspace.getConfiguration(TelemetryReporter.TELEMETRY_SECTION_ID);
        var isTelemetryEnabled = telemetrySettings.get(TelemetryReporter.TELEMETRY_OLD_SETTING_ID);
        // Only available since https://code.visualstudio.com/updates/v1_61#_telemetry-settings
        var telemetryLevel = telemetrySettings.get(TelemetryReporter.TELEMETRY_SETTING_ID);
        // `enableTelemetry` is either true or false (default = true). Deprecated since https://code.visualstudio.com/updates/v1_61#_telemetry-settings
        // It is replaced by `telemetryLevel`, only available since v1.61 (default = 'all')
        // https://code.visualstudio.com/docs/getstarted/telemetry
        // To enable Telemetry:
        // We check that
        // `enableTelemetry` is true and `telemetryLevel` falsy -> enabled
        // `enableTelemetry` is true and `telemetryLevel` set to 'all' -> enabled
        // anything else falls back to disabled.
        var isTelemetryEnabledWithOldSetting = isTelemetryEnabled && !telemetryLevel;
        var isTelemetryEnabledWithNewSetting = isTelemetryEnabled && telemetryLevel && telemetryLevel === 'all';
        if (isTelemetryEnabledWithOldSetting || isTelemetryEnabledWithNewSetting) {
            this.userOptIn = true;
            console.info('Telemetry is enabled for BAML extension');
        }
        else {
            this.userOptIn = false;
            console.info('Telemetry is disabled for BAML extension');
        }
    };
    TelemetryReporter.prototype.dispose = function () {
        return __awaiter(this, void 0, void 0, function () {
            return __generator(this, function (_a) {
                switch (_a.label) {
                    case 0:
                        this.configListener.dispose();
                        return [4 /*yield*/, client.shutdownAsync()];
                    case 1:
                        _a.sent();
                        return [2 /*return*/];
                }
            });
        });
    };
    TelemetryReporter.TELEMETRY_SECTION_ID = 'telemetry';
    TelemetryReporter.TELEMETRY_SETTING_ID = 'telemetryLevel';
    // Deprecated since https://code.visualstudio.com/updates/v1_61#_telemetry-settings
    TelemetryReporter.TELEMETRY_OLD_SETTING_ID = 'enableTelemetry';
    return TelemetryReporter;
}());
exports.default = TelemetryReporter;
