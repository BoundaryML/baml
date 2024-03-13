"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.relay = exports.on = exports.send = void 0;
/**
 * Checks if the given message is an internal node-dev message.
 */
function isNodeDevMessage(m) {
    return m.cmd === 'NODE_DEV';
}
/**
 * Sends a message to the given process.
 */
exports.send = function (m, dest) {
    if (dest === void 0) { dest = process; }
    m.cmd = 'NODE_DEV';
    if (dest.send)
        dest.send(m);
};
exports.on = function (proc, type, cb) {
    function handleMessage(m) {
        if (isNodeDevMessage(m) && type in m)
            cb(m);
    }
    proc.on('internalMessage', handleMessage);
    proc.on('message', handleMessage);
};
exports.relay = function (src, dest) {
    if (dest === void 0) { dest = process; }
    function relayMessage(m) {
        if (isNodeDevMessage(m))
            dest.send(m);
    }
    src.on('internalMessage', relayMessage);
    src.on('message', relayMessage);
};
