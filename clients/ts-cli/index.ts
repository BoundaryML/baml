import { program } from '@commander-js/extra-typings'
import * as baml from '@boundaryml/baml'

program.name('baml-cli').version('0.1.0').description('BAML CLI')

// Define the 'generate' subcommand
program
  .command('generate')
  .description('Generate a BAML client from a baml_src dir')
  .requiredOption('--from <path/to/baml_src>', 'The baml_src directory to generate the client from')
  .requiredOption('--to <output_path>', 'The output path where the client will be generated')
  .action((options) => {
    // TODO: allow generating any client type through the TS CLI
    const clientType = 'typescript'
    console.info(`Generating ${clientType} BAML client in ${options.to} from ${options.from}...`)
    baml.BamlRuntimeFfi.fromDirectory(options.from).generateClient({
      clientType: baml.LanguageClientType.Typescript,
      outputPath: options.to,
    })
  })

program.parse()
