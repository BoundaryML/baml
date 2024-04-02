"use strict";
var __importDefault = (this && this.__importDefault) || function (mod) {
    return (mod && mod.__esModule) ? mod : { "default": mod };
};
Object.defineProperty(exports, "__esModule", { value: true });
const baml_core_1 = require("@boundaryml/baml-core");
const baml_client_1 = __importDefault(require("../baml_client"));
class MinRepro {
    //[Symbol.asyncIterator] = async function* () {
    constructor(words) {
        this.words = words;
    }
    [Symbol.asyncIterator]() {
        const words = [...this.words];
        return {
            next: async () => {
                const word = words.shift();
                if (word === undefined) {
                    return { value: undefined, done: true };
                }
                return { value: word, done: false };
            }
        };
    }
}
const testCompilation = async () => {
    async function* events() {
        yield {
            generated: "llm1",
            model_name: "model",
            meta: {},
        };
        yield {
            generated: "llm2",
            model_name: "model",
            meta: {},
        };
        yield {
            generated: "llm3",
            model_name: "model",
            meta: {},
        };
    }
    const llm = new baml_core_1.LLMResponseStream(events(), (partial) => null, (final) => final.generated);
    console.log("testing compile");
    for await (const result of llm) {
        console.log(JSON.stringify(result, null, 2));
    }
};
const main = async () => {
    console.log("Hello, World!");
    const repro = new MinRepro(["lorem", "ipsum"]);
    for await (const word of repro) {
        console.log(word);
    }
    //const result = await b.OptionalTest_Function.stream("Hello, World!");
    const stream = baml_client_1.default.OptionalTest_Function.getImpl("v1").stream("Hello, World!");
    for await (const result of stream) {
        console.log(JSON.stringify(result, null, 2));
    }
};
if (require.main === module) {
    testCompilation();
    main();
}
