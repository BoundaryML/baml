"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var extra_typings_1 = require("@commander-js/extra-typings");
var baml = require("@boundaryml/baml");
extra_typings_1.program.name('baml-cli').version('0.1.0').description('BAML CLI');
// Define the 'generate' subcommand
extra_typings_1.program
    .command('generate')
    .description('Generate a BAML client from a baml_src dir')
    .requiredOption('--from <path/to/baml_src>', 'The baml_src directory to generate the client from')
    .requiredOption('--to <output_path>', 'The output path where the client will be generated')
    .action(function (options) {
    // TODO: allow generating any client type through the TS CLI
    var clientType = 'typescript';
    console.info("Generating ".concat(clientType, " BAML client in ").concat(options.to, " from ").concat(options.from, "..."));
    baml.BamlRuntimeFfi.fromDirectory(options.from).generateClient({
        clientType: baml.LanguageClientType.Typescript,
        outputPath: options.to,
    });
});
extra_typings_1.program.parse();
