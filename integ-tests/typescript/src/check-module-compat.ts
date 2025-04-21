import { build } from 'esbuild';
import { readdirSync, statSync, readFileSync } from 'fs';
import { join, extname, dirname } from 'path';

type ModuleFormat = 'esm' | 'cjs';
type Failure = { file: string; message: string };

/**
 * Recursively finds all script files (ts, js, mjs, cjs, etc.) in a directory.
 */
export function findScriptFiles(dir: string, _currentFiles: string[] = []): string[] {
  try {
    // Initial check for the top-level directory
    if (_currentFiles.length === 0 && !statSync(dir).isDirectory()) {
      throw new Error(`Provided path is not a directory: ${dir}`);
    }

    for (const entry of readdirSync(dir)) {
      const fullPath = join(dir, entry);
      try {
        if (statSync(fullPath).isDirectory()) {
          findScriptFiles(fullPath, _currentFiles); // Recurse
        } else if (/\.(ts|js|mts|cts|mjs|cjs)$/.test(extname(fullPath))) {
          _currentFiles.push(fullPath);
        }
      } catch (error) {
        // Log errors accessing specific files/subdirs but continue
        console.warn(`⚠️ Skipping ${fullPath}: ${error instanceof Error ? error.message : String(error)}`);
      }
    }
  } catch (error) {
    // Throw errors related to accessing the root directory
    console.error(`❌ Error accessing directory: ${dir}`, error);
    throw error; // Re-throw for the caller to handle
  }
  return _currentFiles;
}


/**
 * Checks module compatibility for a list of files using esbuild.
 */
export async function checkFilesCompatibility(
  filesToCheck: string[],
  format: ModuleFormat,
  rootDir: string // Used for context in error messages
): Promise<Failure[]> {
  const failures: Failure[] = [];
  const relativeImportExportRegex = /(?:import|export).*from\s+(['"])(\.\.?\/[^'"\n]+)\1/g;
  const allowedExtensionsRegex = /\.(?:ts|js|mts|cts|mjs|cjs)$/i;

  for (const file of filesToCheck) {
    let hasExtensionlessImport = false;
    try {
      // 1. Check for extensionless relative imports/exports first
      const content = readFileSync(file, 'utf-8');
      let match;
      while ((match = relativeImportExportRegex.exec(content)) !== null) {
        const importPath = match[2];
        if (!allowedExtensionsRegex.test(importPath)) {
          // Check if it resolves to an index file - this is often allowed implicitly
          try {
            const resolvedPath = join(dirname(file), importPath);
            if (!statSync(resolvedPath).isDirectory()) {
              // Only fail if it's not resolving to a directory (which implies index.*)
              failures.push({
                file,
                message: `Relative import/export path lacks extension: "${importPath}"`,
              });
              hasExtensionlessImport = true;
              break; // Stop checking this file after first violation
            }
          } catch (resolveError) {
              // If resolving the path itself fails, it's likely an invalid import anyway
              // Let esbuild handle this, but also mark it as extensionless if it lacked one
              failures.push({
                file,
                message: `Relative import/export path lacks extension: "${importPath}"`,
              });
              hasExtensionlessImport = true;
              break; // Stop checking this file after first violation
          }
        }
      }

      // 2. Run esbuild check (only if no extensionless import found, or run always?)
      // Let's run esbuild always to catch other errors, but report extension error regardless.
      await build({
        entryPoints: [file],
        bundle: true,
        format,
        platform: 'node',
        outfile: '/dev/null',
        write: false,
        logLevel: 'silent',
        loader: {
          ".node": "file",
        }
      });
    } catch (err) {
      const message = (err instanceof Error ? err.message : String(err)).trim();
      // Avoid adding duplicate generic build error if we already added a specific extension error
      if (!hasExtensionlessImport || !failures.some(f => f.file === file)) {
        failures.push({ file, message });
      }
    }
  }
  return failures;
}

// --- Main Execution & CLI Handling ---

interface CheckOptions {
  format: ModuleFormat;
  rootDir: string;
}

interface CheckResult {
  success: boolean;
  checkedFiles: string[];
  failures: Failure[];
  options: CheckOptions;
}

/**
 * Finds script files and checks their module compatibility.
 */
export async function runCompatibilityCheck(options: CheckOptions): Promise<CheckResult> {
  const { rootDir, format } = options;
  let checkedFiles: string[] = [];
  let failures: Failure[] = [];
  let success = false;

  try {
    checkedFiles = findScriptFiles(rootDir);
    if (checkedFiles.length > 0) {
        console.log(`Found ${checkedFiles.length} files to check in ${rootDir}.`);
        failures = await checkFilesCompatibility(checkedFiles, format, rootDir);
        success = failures.length === 0;
    } else {
        console.log(`No script files found in ${rootDir}.`);
        success = true; // No files found is considered success
    }
  } catch (error) {
      // Error during file finding (already logged by findScriptFiles)
      success = false;
      // Optionally add a general failure if needed, though specific error is logged
      // failures.push({ file: rootDir, message: `Error finding files: ${error instanceof Error ? error.message : String(error)}` });
  }


  return { success, checkedFiles, failures, options };
}

// Function to parse CLI args (similar to before, but returns options)
function parseCliArgs(): CheckOptions | null {
    const format = process.argv.includes('--esm')
        ? 'esm'
        : process.argv.includes('--cjs')
          ? 'cjs'
          : null;

    if (!format) {
        console.error('❌ Missing flag: use --esm or --cjs');
        return null;
    }

    const dirArgIndex = process.argv.indexOf('--dir');
    let rootDir = '.';
    if (dirArgIndex > -1 && process.argv.length > dirArgIndex + 1) {
        rootDir = process.argv[dirArgIndex + 1];
    } else {
        console.warn(`⚠️ No --dir specified, defaulting to current directory (${rootDir})`);
    }

    return { format, rootDir };
}

// Run only if executed directly from CLI
// Using a simple check that works in most Node environments (CJS/ESM)
if (require.main === module || (typeof process.versions.bun !== 'undefined' && process.argv[1] === __filename) || (process.argv[1] && process.argv[1].endsWith('check-module-compat.ts'))) {
    (async () => {
        const options = parseCliArgs();
        if (!options) {
            process.exit(1);
        }

        const result = await runCompatibilityCheck(options);

        if (!result.success) {
            if (result.failures.length > 0) {
                console.error(`\n❌ ${result.failures.length} file(s) in ${result.options.rootDir} are not ${result.options.format}-compatible:\n`);
                for (const { file, message } of result.failures) {
                    console.error(`— ${file}`);
                    console.error(`  ${message.replace(/\n/g, '\n  ')}`);
                    console.error('');
                }
            } else {
                // Handle cases where finding files failed (error already logged)
                 console.error(`\n❌ Failed to complete compatibility check for ${result.options.rootDir}. See errors above.`);
            }
            process.exit(1);
        } else {
            console.log(`✅ All ${result.checkedFiles.length} files checked in ${result.options.rootDir} are ${result.options.format}-compatible`);
        }
    })();
}
