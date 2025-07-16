"use strict";
var __createBinding = (this && this.__createBinding) || (Object.create ? (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    var desc = Object.getOwnPropertyDescriptor(m, k);
    if (!desc || ("get" in desc ? !m.__esModule : desc.writable || desc.configurable)) {
      desc = { enumerable: true, get: function() { return m[k]; } };
    }
    Object.defineProperty(o, k2, desc);
}) : (function(o, m, k, k2) {
    if (k2 === undefined) k2 = k;
    o[k2] = m[k];
}));
var __exportStar = (this && this.__exportStar) || function(m, exports) {
    for (var p in m) if (p !== "default" && !Object.prototype.hasOwnProperty.call(exports, p)) __createBinding(exports, m, p);
};
Object.defineProperty(exports, "__esModule", { value: true });
exports.BamlCtxManager = exports.BamlStream = exports.TraceStats = exports.Timing = exports.StreamTiming = exports.SSEResponse = exports.HTTPResponse = exports.HTTPRequest = exports.Usage = exports.FunctionLog = exports.Collector = exports.BamlLogEvent = exports.ClientRegistry = exports.invoke_runtime_cli = exports.Video = exports.Pdf = exports.Audio = exports.Image = exports.FunctionResultStream = exports.FunctionResult = exports.BamlRuntime = void 0;
__exportStar(require("./safe_imports"), exports);
__exportStar(require("./errors"), exports);
__exportStar(require("./logging"), exports);
// Detect if we're in a Node.js environment
const isNode = typeof process !== 'undefined' && process.versions != null && process.versions.node != null;
if (!isNode) {
    const browserError = (name) => {
        throw new Error(`Cannot import ${name} from '@boundaryml/baml' in browser environment. Please import from '@boundaryml/baml/browser' instead.`);
    };
    // Provide helpful error messages for browser imports
    Object.defineProperty(exports, 'Image', {
        get: () => browserError('Image'),
        enumerable: true,
    });
    Object.defineProperty(exports, 'Audio', {
        get: () => browserError('Audio'),
        enumerable: true,
    });
    Object.defineProperty(exports, 'Pdf', {
        get: () => browserError('Pdf'),
        enumerable: true,
    });
    Object.defineProperty(exports, 'Video', {
        get: () => browserError('Video'),
        enumerable: true,
    });
}
var native_1 = require("./native");
Object.defineProperty(exports, "BamlRuntime", { enumerable: true, get: function () { return native_1.BamlRuntime; } });
Object.defineProperty(exports, "FunctionResult", { enumerable: true, get: function () { return native_1.FunctionResult; } });
Object.defineProperty(exports, "FunctionResultStream", { enumerable: true, get: function () { return native_1.FunctionResultStream; } });
Object.defineProperty(exports, "Image", { enumerable: true, get: function () { return native_1.BamlImage; } });
Object.defineProperty(exports, "Audio", { enumerable: true, get: function () { return native_1.BamlAudio; } });
Object.defineProperty(exports, "Pdf", { enumerable: true, get: function () { return native_1.BamlPdf; } });
Object.defineProperty(exports, "Video", { enumerable: true, get: function () { return native_1.BamlVideo; } });
Object.defineProperty(exports, "invoke_runtime_cli", { enumerable: true, get: function () { return native_1.invoke_runtime_cli; } });
Object.defineProperty(exports, "ClientRegistry", { enumerable: true, get: function () { return native_1.ClientRegistry; } });
Object.defineProperty(exports, "BamlLogEvent", { enumerable: true, get: function () { return native_1.BamlLogEvent; } });
Object.defineProperty(exports, "Collector", { enumerable: true, get: function () { return native_1.Collector; } });
Object.defineProperty(exports, "FunctionLog", { enumerable: true, get: function () { return native_1.FunctionLog; } });
Object.defineProperty(exports, "Usage", { enumerable: true, get: function () { return native_1.Usage; } });
Object.defineProperty(exports, "HTTPRequest", { enumerable: true, get: function () { return native_1.HTTPRequest; } });
Object.defineProperty(exports, "HTTPResponse", { enumerable: true, get: function () { return native_1.HTTPResponse; } });
Object.defineProperty(exports, "SSEResponse", { enumerable: true, get: function () { return native_1.SSEResponse; } });
Object.defineProperty(exports, "StreamTiming", { enumerable: true, get: function () { return native_1.StreamTiming; } });
Object.defineProperty(exports, "Timing", { enumerable: true, get: function () { return native_1.Timing; } });
Object.defineProperty(exports, "TraceStats", { enumerable: true, get: function () { return native_1.TraceStats; } });
var stream_1 = require("./stream");
Object.defineProperty(exports, "BamlStream", { enumerable: true, get: function () { return stream_1.BamlStream; } });
var async_context_vars_1 = require("./async_context_vars");
Object.defineProperty(exports, "BamlCtxManager", { enumerable: true, get: function () { return async_context_vars_1.BamlCtxManager; } });
