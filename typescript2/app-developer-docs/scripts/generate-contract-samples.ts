import { generateCliPublicationInput } from '@/lib/generated-content/cli-generator';
import {
  canonicalJson,
  jsonValueSchema,
  sha256,
} from '@/lib/generated-content/json';
import {
  generatePackagePublicationInput,
  type ReferencePageProjection,
} from '@/lib/generated-content/package-generator';
import {
  parseOperatorArguments,
  requireOperatorValue,
} from '@/scripts/operator-arguments';

function requirePage(
  pages: ReferencePageProjection[],
  qualifiedName: string,
): ReferencePageProjection {
  const page = pages.find(
    (candidate) => candidate.qualifiedName === qualifiedName,
  );
  if (!page) {
    throw new Error(
      `Representative page ${qualifiedName} is missing from the export.`,
    );
  }
  return page;
}

async function main(): Promise<void> {
  const parsedArguments = parseOperatorArguments(
    process.argv.slice(2),
    ['baml-bin'],
    ['print-samples'],
  );
  const bamlBinary = requireOperatorValue(parsedArguments, 'baml-bin');
  const [packageInput, cliInput] = await Promise.all([
    generatePackagePublicationInput(bamlBinary, 'baml'),
    generateCliPublicationInput(bamlBinary),
  ]);

  const packagePage = requirePage(packageInput.pages, 'baml');
  const namespacePage = requirePage(packageInput.pages, 'baml.csv');
  const declarationPage = requirePage(packageInput.pages, 'baml.Array');
  const functionPage = requirePage(packageInput.pages, 'baml.csv.parse');
  const cliSubtree = cliInput.payload.root.subcommands.find(
    (command) => command.name === 'generate',
  );
  if (!cliSubtree || cliSubtree.subcommands.length === 0) {
    throw new Error('Representative nested CLI generate subtree is missing.');
  }

  const samples = {
    cli_root_with_generate_subtree: {
      ...cliInput.payload.root,
      subcommands: [cliSubtree],
    },
    declaration_page_data: declarationPage.pageData,
    function_page_data: functionPage.pageData,
    namespace_page_data: namespacePage.pageData,
    package_page_data: packagePage.pageData,
  };
  const summary = {
    binary_identity: {
      product_version: cliInput.productVersion,
      wrapper_version: cliInput.wrapperVersion,
    },
    cli: {
      captured_help_entries: cliInput.payload.raw_help.length,
      payload_sha256: cliInput.payloadSha256,
      root_commands: cliInput.payload.root.subcommands.length,
      source_sha256: cliInput.sourceSha256,
    },
    package: {
      describe_format_version: packageInput.describeFormatVersion,
      describe_sha256: packageInput.describeSha256,
      projected_pages: packageInput.pages.length,
      projected_pages_by_kind: Object.fromEntries(
        Object.entries(
          Object.groupBy(packageInput.pages, (page) => page.pageKind),
        ).map(([kind, pages]) => [kind, pages.length]),
      ),
    },
    representative_sample_sha256: Object.fromEntries(
      Object.entries(samples).map(([name, value]) => [
        name,
        sha256(canonicalJson(jsonValueSchema.parse(value))),
      ]),
    ),
  };

  console.log(JSON.stringify(summary, null, 2));
  if (parsedArguments.flags.has('print-samples')) {
    console.log(JSON.stringify({ representative_samples: samples }, null, 2));
  }
}

main().catch((cause: unknown) => {
  console.error(
    cause instanceof Error
      ? cause.message
      : 'Unknown contract-sample generation failure.',
  );
  process.exitCode = 1;
});
