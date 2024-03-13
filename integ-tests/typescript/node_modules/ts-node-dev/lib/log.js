"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.makeLog = void 0;
var util = require('util');
var colors = {
    info: '36',
    error: '31;1',
    warn: '33',
    debug: '90',
};
var formatDate = function (date) {
    return [date.getHours(), date.getMinutes(), date.getSeconds()]
        .map(function (n) { return n.toString().padStart(2, '0'); })
        .join(':');
};
/**
 * Logs a message to the console. The level is displayed in ANSI colors,
 * either bright red in case of an error or green otherwise.
 */
exports.makeLog = function (cfg) {
    function log(msg, level) {
        if (cfg.quiet && level === 'info')
            return;
        if (cfg.timestamp)
            msg = color(formatDate(new Date()), '30;1') + ' ' + msg;
        var c = colors[level.toLowerCase()] || '32';
        console.log('[' + color(level.toUpperCase(), c) + '] ' + msg);
    }
    function color(s, c) {
        if (process.stdout.isTTY) {
            return '\x1B[' + c + 'm' + s + '\x1B[0m';
        }
        return s;
    }
    log.debug = function () {
        if (!cfg.debug)
            return;
        log(util.format.apply(util, arguments), 'debug');
    };
    log.info = function () {
        log(util.format.apply(util, arguments), 'info');
    };
    log.warn = function () {
        log(util.format.apply(util, arguments), 'warn');
    };
    log.error = function () {
        log(util.format.apply(util, arguments), 'error');
    };
    return log;
};
