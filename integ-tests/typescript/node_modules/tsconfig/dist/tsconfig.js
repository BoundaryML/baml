"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var fs = require("fs");
var path = require("path");
var stripBom = require("strip-bom");
var stripComments = require("strip-json-comments");
var CONFIG_FILENAME = 'tsconfig.json';
function resolve(cwd, filename) {
    if (!filename) {
        return find(cwd);
    }
    var fullPath = path.resolve(cwd, filename);
    return stat(fullPath)
        .then(function (stats) {
        if (isFile(stats)) {
            return fullPath;
        }
        if (isDirectory(stats)) {
            var configFile_1 = path.join(fullPath, CONFIG_FILENAME);
            return stat(configFile_1)
                .then(function (stats) {
                if (isFile(stats)) {
                    return configFile_1;
                }
                throw new TypeError("Cannot find a " + CONFIG_FILENAME + " file at the specified directory: " + filename);
            });
        }
        throw new TypeError("The specified path does not exist: " + filename);
    });
}
exports.resolve = resolve;
function resolveSync(cwd, filename) {
    if (!filename) {
        return findSync(cwd);
    }
    var fullPath = path.resolve(cwd, filename);
    var stats = statSync(fullPath);
    if (isFile(stats)) {
        return fullPath;
    }
    if (isDirectory(stats)) {
        var configFile = path.join(fullPath, CONFIG_FILENAME);
        var stats_1 = statSync(configFile);
        if (isFile(stats_1)) {
            return configFile;
        }
        throw new TypeError("Cannot find a " + CONFIG_FILENAME + " file at the specified directory: " + filename);
    }
    throw new TypeError("The specified path does not exist: " + filename);
}
exports.resolveSync = resolveSync;
function find(dir) {
    var configFile = path.resolve(dir, CONFIG_FILENAME);
    return stat(configFile)
        .then(function (stats) {
        if (isFile(stats)) {
            return configFile;
        }
        var parentDir = path.dirname(dir);
        if (dir === parentDir) {
            return;
        }
        return find(parentDir);
    });
}
exports.find = find;
function findSync(dir) {
    var configFile = path.resolve(dir, CONFIG_FILENAME);
    var stats = statSync(configFile);
    if (isFile(stats)) {
        return configFile;
    }
    var parentDir = path.dirname(dir);
    if (dir === parentDir) {
        return;
    }
    return findSync(parentDir);
}
exports.findSync = findSync;
function load(cwd, filename) {
    return resolve(cwd, filename)
        .then(function (path) {
        if (path == null) {
            return Promise.resolve({
                config: {
                    files: [],
                    compilerOptions: {}
                }
            });
        }
        return readFile(path).then(function (config) { return ({ path: path, config: config }); });
    });
}
exports.load = load;
function loadSync(cwd, filename) {
    var path = resolveSync(cwd, filename);
    if (path == null) {
        return {
            config: {
                files: [],
                compilerOptions: {}
            }
        };
    }
    var config = readFileSync(path);
    return { path: path, config: config };
}
exports.loadSync = loadSync;
function readFile(filename) {
    return new Promise(function (resolve, reject) {
        fs.readFile(filename, 'utf8', function (err, contents) {
            if (err) {
                return reject(err);
            }
            try {
                return resolve(parse(contents, filename));
            }
            catch (err) {
                return reject(err);
            }
        });
    });
}
exports.readFile = readFile;
function readFileSync(filename) {
    var contents = fs.readFileSync(filename, 'utf8');
    return parse(contents, filename);
}
exports.readFileSync = readFileSync;
function parse(contents, filename) {
    var data = stripComments(stripBom(contents));
    if (/^\s*$/.test(data)) {
        return {};
    }
    return JSON.parse(data);
}
exports.parse = parse;
function stat(filename) {
    return new Promise(function (resolve, reject) {
        fs.stat(filename, function (err, stats) {
            return err ? resolve(undefined) : resolve(stats);
        });
    });
}
function statSync(filename) {
    try {
        return fs.statSync(filename);
    }
    catch (e) {
        return;
    }
}
function isFile(stats) {
    return stats ? stats.isFile() || stats.isFIFO() : false;
}
function isDirectory(stats) {
    return stats ? stats.isDirectory() : false;
}
//# sourceMappingURL=tsconfig.js.map