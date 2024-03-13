"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.resolveMain = void 0;
var resolve = require('resolve');
function resolveRequest(req) {
    // The `resolve` package is prebuilt through ncc, which prevents
    // PnP from being able to inject itself into it. To circumvent
    // this, we simply use PnP directly when available.
    if (process.versions.pnp) {
        var resolveRequest_1 = require("pnpapi").resolveRequest;
        return resolveRequest_1(req, process.cwd() + '/');
    }
    else {
        var opts = {
            basedir: process.cwd(),
            paths: [process.cwd()],
        };
        return resolve.sync(req, opts);
    }
}
exports.resolveMain = function (main) {
    try {
        return resolveRequest(main + '.ts');
    }
    catch (e) {
        try {
            return resolveRequest(main + '/index.ts');
        }
        catch (e) {
            return resolveRequest(main);
        }
    }
};
