"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var chai_1 = require("chai");
var path_1 = require("path");
var util_1 = require("util");
var tsconfig = require("./tsconfig");
var TEST_DIR = path_1.join(__dirname, '../tests');
describe('tsconfig', function () {
    var tests = [
        {
            args: [TEST_DIR, 'invalidfile'],
            error: parseInt(process.versions.node, 10) > 5 ? 'Unexpected token s in JSON at position 0' : 'Unexpected token s'
        },
        {
            args: [TEST_DIR, 'missing'],
            error: 'Cannot find a tsconfig.json file at the specified directory: missing'
        },
        {
            args: [TEST_DIR, 'missing/foobar'],
            error: 'The specified path does not exist: missing/foobar'
        },
        {
            args: ['/'],
            config: {
                files: [],
                compilerOptions: {}
            }
        },
        {
            args: [TEST_DIR, 'empty'],
            config: {},
            path: path_1.join(TEST_DIR, 'empty/tsconfig.json')
        },
        {
            args: [TEST_DIR, 'empty/tsconfig.json'],
            config: {},
            path: path_1.join(TEST_DIR, 'empty/tsconfig.json')
        },
        {
            args: [path_1.join(TEST_DIR, 'find/up/config')],
            config: {},
            path: path_1.join(TEST_DIR, 'find/tsconfig.json')
        },
        {
            args: [TEST_DIR, 'valid'],
            config: {
                compilerOptions: {
                    module: 'commonjs',
                    noImplicitAny: true,
                    outDir: 'dist',
                    removeComments: true,
                    sourceMap: true,
                    preserveConstEnums: true
                },
                files: [
                    './src/foo.ts'
                ]
            },
            path: path_1.join(TEST_DIR, 'valid/tsconfig.json')
        },
        {
            args: [TEST_DIR, 'bom'],
            config: {
                compilerOptions: {
                    module: 'commonjs',
                    noImplicitAny: true,
                    outDir: 'dist',
                    removeComments: true,
                    sourceMap: true,
                    preserveConstEnums: true
                },
                files: [
                    './src/bom.ts'
                ]
            },
            path: path_1.join(TEST_DIR, 'bom/tsconfig.json')
        },
        {
            args: [path_1.join(TEST_DIR, 'cwd')],
            config: {
                compilerOptions: {
                    module: 'commonjs',
                    noImplicitAny: true,
                    outDir: 'dist',
                    removeComments: true,
                    sourceMap: true,
                    preserveConstEnums: true
                }
            },
            path: path_1.join(TEST_DIR, 'cwd/tsconfig.json')
        }
    ];
    describe('sync', function () {
        tests.forEach(function (test) {
            describe(util_1.inspect(test.args), function () {
                it('should try to find config', function () {
                    var result;
                    try {
                        result = tsconfig.loadSync(test.args[0], test.args[1]);
                    }
                    catch (err) {
                        chai_1.expect(err.message).to.equal(test.error);
                        return;
                    }
                    chai_1.expect(result.path).to.equal(test.path);
                    chai_1.expect(result.config).to.deep.equal(test.config);
                });
                if (test.path) {
                    it('should resolve filename', function () {
                        chai_1.expect(tsconfig.resolveSync(test.args[0], test.args[1])).to.equal(test.path);
                    });
                }
            });
        });
    });
    describe('async', function () {
        tests.forEach(function (test) {
            describe(util_1.inspect(test.args), function () {
                it('should try to find config', function () {
                    return tsconfig.load(test.args[0], test.args[1])
                        .then(function (result) {
                        chai_1.expect(result.path).to.equal(test.path);
                        chai_1.expect(result.config).to.deep.equal(test.config);
                    }, function (error) {
                        chai_1.expect(error.message).to.equal(test.error);
                    });
                });
                if (test.path) {
                    it('should resolve filename', function () {
                        return tsconfig.resolve(test.args[0], test.args[1])
                            .then(function (filename) {
                            chai_1.expect(filename).to.equal(test.path);
                        });
                    });
                }
            });
        });
    });
});
//# sourceMappingURL=tsconfig.spec.js.map