"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
var fs_1 = __importDefault(require("fs"));
var filePath = process.argv[2];
var handler = function (stat) {
    if (stat && stat.birthtime.getTime() > 0) {
        process.exit(0);
    }
};
fs_1.default.watchFile(filePath, { interval: 100 }, handler);
fs_1.default.stat(filePath, function (err, stat) {
    handler(stat);
});
