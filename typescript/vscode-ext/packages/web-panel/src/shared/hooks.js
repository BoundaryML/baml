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
Object.defineProperty(exports, "__esModule", { value: true });
exports.useImplCtx = exports.useSelections = void 0;
var react_1 = require("react");
var ASTProvider_1 = require("./ASTProvider");
function removeUnreferencedDefinitions(schema) {
    if (!schema || typeof schema !== 'object' || !schema.definitions) {
        return schema;
    }
    // Function to collect references from a given object
    function collectRefs(obj, refs) {
        if (obj && typeof obj === 'object') {
            for (var _i = 0, _a = Object.keys(obj); _i < _a.length; _i++) {
                var key = _a[_i];
                if (key === '$ref' && typeof obj[key] === 'string') {
                    // Extract and store the reference
                    var ref = obj[key].replace('#/definitions/', '');
                    refs.add(ref);
                }
                else {
                    // Recursively collect references from nested objects
                    collectRefs(obj[key], refs);
                }
            }
        }
    }
    // Initialize a set to keep track of all referenced definitions
    var referencedDefs = new Set();
    // Collect references from the entire schema, excluding the definitions object itself
    collectRefs(__assign(__assign({}, schema), { definitions: {} }), referencedDefs);
    // Iterate over the definitions to find and include indirectly referenced definitions
    var newlyAdded;
    do {
        newlyAdded = false;
        for (var _i = 0, _a = Object.keys(schema.definitions); _i < _a.length; _i++) {
            var def = _a[_i];
            if (referencedDefs.has(def)) {
                var initialSize = referencedDefs.size;
                collectRefs(schema.definitions[def], referencedDefs);
                if (referencedDefs.size > initialSize) {
                    newlyAdded = true;
                }
            }
        }
    } while (newlyAdded);
    // Filter out definitions that are not referenced
    var newDefinitions = Object.keys(schema.definitions)
        .filter(function (def) { return referencedDefs.has(def); })
        .reduce(function (newDefs, def) {
        if (schema.definitions) {
            newDefs[def] = schema.definitions[def];
        }
        return newDefs;
    }, {});
    return __assign(__assign({}, schema), { definitions: newDefinitions });
}
function useSelections() {
    var ctx = (0, react_1.useContext)(ASTProvider_1.ASTContext);
    if (!ctx) {
        throw new Error('useSelections must be used within an ASTProvider');
    }
    var db = ctx.db, test_results_raw = ctx.test_results, jsonSchema = ctx.jsonSchema, test_log = ctx.test_log, _a = ctx.selections, selectedFunction = _a.selectedFunction, selectedImpl = _a.selectedImpl, selectedTestCase = _a.selectedTestCase, showTests = _a.showTests;
    var func = (0, react_1.useMemo)(function () {
        if (!selectedFunction) {
            return db.functions.at(0);
        }
        return db.functions.find(function (f) { return f.name.value === selectedFunction; });
    }, [db.functions, selectedFunction]);
    var impl = (0, react_1.useMemo)(function () {
        if (!func) {
            return undefined;
        }
        if (!selectedImpl) {
            return func.impls.at(0);
        }
        return func.impls.find(function (i) { return i.name.value === selectedImpl; });
    }, [func, selectedImpl]);
    var test_case = (0, react_1.useMemo)(function () {
        var _a;
        if (selectedTestCase === '<new>') {
            return undefined;
        }
        return (_a = func === null || func === void 0 ? void 0 : func.test_cases.find(function (t) { return t.name.value === selectedTestCase; })) !== null && _a !== void 0 ? _a : func === null || func === void 0 ? void 0 : func.test_cases.at(0);
    }, [func, selectedTestCase]);
    // TODO: we should just publish a global test status instead of relying
    // on this exit code.
    var test_result_exit_status = (0, react_1.useMemo)(function () {
        if (!test_results_raw)
            return "NOT_STARTED";
        return test_results_raw.run_status;
    }, [test_results_raw, func === null || func === void 0 ? void 0 : func.name.value]);
    var test_result_url = (0, react_1.useMemo)(function () {
        if (!test_results_raw)
            return undefined;
        if (test_results_raw.test_url) {
            return { text: 'Dashboard', url: test_results_raw.test_url };
        }
        else {
            return {
                text: 'Learn how to persist runs',
                url: 'https://docs.boundaryml.com/v2/mdx/quickstart#setting-up-the-boundary-dashboard',
            };
        }
    }, [test_results_raw, func === null || func === void 0 ? void 0 : func.name.value]);
    var test_results = (0, react_1.useMemo)(function () {
        return test_results_raw === null || test_results_raw === void 0 ? void 0 : test_results_raw.results.filter(function (tr) { return tr.functionName == (func === null || func === void 0 ? void 0 : func.name.value); }).map(function (tr) {
            var relatedTest = func === null || func === void 0 ? void 0 : func.test_cases.find(function (tc) { return tc.name.value == tr.testName; });
            return __assign(__assign({}, tr), { input: relatedTest === null || relatedTest === void 0 ? void 0 : relatedTest.content, span: relatedTest === null || relatedTest === void 0 ? void 0 : relatedTest.name });
        });
    }, [test_results_raw, func === null || func === void 0 ? void 0 : func.name.value]);
    var input_json_schema = (0, react_1.useMemo)(function () {
        if (!func)
            return undefined;
        var base_schema = __assign({ title: "".concat(func.name.value, " Input") }, jsonSchema);
        var merged_schema = {};
        if (func.input.arg_type === 'named') {
            merged_schema = __assign({ type: 'object', properties: Object.fromEntries(func.input.values.map(function (v) { return [v.name.value, v.jsonSchema]; })) }, base_schema);
        }
        else {
            merged_schema = __assign(__assign({}, func.input.jsonSchema), base_schema);
        }
        return removeUnreferencedDefinitions(merged_schema);
    }, [func, jsonSchema]);
    return {
        func: func,
        impl: impl,
        showTests: showTests,
        test_case: test_case,
        test_results: test_results,
        test_result_url: test_result_url,
        test_result_exit_status: test_result_exit_status,
        test_log: test_log,
        input_json_schema: input_json_schema,
    };
}
exports.useSelections = useSelections;
function useImplCtx(name) {
    var func = useSelections().func;
    return { func: func };
}
exports.useImplCtx = useImplCtx;
