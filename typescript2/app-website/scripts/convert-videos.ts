#!/usr/bin/env tsx

import { existsSync } from 'fs';
import { statSync } from 'fs';
import os from 'os';
import path from 'path';
import type { LogMessageOptions } from '@clack/prompts';
import {
  log as clackLog,
  confirm,
  group,
  intro,
  isCancel,
  multiselect,
  note,
  outro,
  select,
  spinner,
  text,
} from '@clack/prompts';
import { glob } from 'glob';
import pc from 'picocolors';
import yargs from 'yargs';
import { hideBin } from 'yargs/helpers';
import { $ } from 'zx';

import dedent from 'dedent-js';

// Types
interface Args {
  verbose: boolean;
  dir?: string;
}

interface FfmpegOptions {
  codec: string;
  crf: number;
  bitrate: number;
  scale: string;
  audio: boolean;
}

interface ConversionResult {
  inputSize: string;
  outputSize: string;
  compressionPercent: string;
  inputFile: string;
  outputFile: string;
}

type ConversionTaskResult =
  | {
      success: true;
      result: ConversionResult;
    }
  | {
      success: false;
      file: string;
      error: string;
    };

// CLI Configuration
const argv = yargs(hideBin(process.argv))
  .option('verbose', {
    alias: 'v',
    type: 'boolean',
    description: 'Run with verbose logging',
    default: false,
  })
  .option('dir', {
    alias: 'd',
    type: 'string',
    description: 'Directory to search for videos',
  })
  .help()
  .parse() as Args;

// Logging utility
const log = {
  info: (message: string) => {
    if (argv.verbose) {
      clackLog.info(message);
    }
  },
  message: (message: string, options: LogMessageOptions) => {
    clackLog.message(message, options);
  },
  step: (message: string) => {
    clackLog.step(message);
  },
  warn: (message: string) => {
    if (argv.verbose) {
      clackLog.warning(message);
    }
  },
  error: (message: string) => {
    clackLog.error(message);
  },
  success: (message: string) => {
    clackLog.success(message);
  },
};

// Make zx silent by default, but enable if verbose
$.verbose = argv.verbose;

// Helper Functions
function handleCancel() {
  outro(pc.red('Operation cancelled'));
  process.exit(0);
}

async function findLatestMediaDirectory(): Promise<string | undefined> {
  const blogDir = path.join(process.cwd(), 'public', 'blog');
  if (!existsSync(blogDir)) return undefined;

  const dirs = (await glob('*', { cwd: blogDir })).filter((p) => {
    // Only include directories that start with a date format YYYY-MM-DD
    return /^\d{4}-\d{2}-\d{2}/.test(p) && existsSync(path.join(blogDir, p));
  });

  if (!dirs.length) return undefined;

  // Sort directories by date prefix (YYYY-MM-DD-*) in descending order
  const sortedDirs = dirs.sort().reverse();
  const latestDir = sortedDirs[0];
  return latestDir ? path.join(blogDir, latestDir) : undefined;
}

async function findVideoFiles(directory?: string): Promise<string[]> {
  let searchDir = directory;

  if (!searchDir) {
    const currentDir = process.cwd();
    const latestMediaDir = await findLatestMediaDirectory();

    const options = [];

    // Add latest blog post first if it exists
    if (latestMediaDir) {
      options.push({
        value: latestMediaDir,
        label: `Latest blog post (${latestMediaDir})`,
      });
    }

    // Add current directory second
    options.push({
      value: currentDir,
      label: `Current directory (${currentDir})`,
    });

    // Add custom directory last
    options.push({ value: 'custom', label: 'Custom directory' });

    const dirChoice = await select({
      message: 'Where would you like to search for videos?',
      options,
    });

    if (isCancel(dirChoice)) handleCancel();

    if (dirChoice === 'custom') {
      const dirInput = await text({
        message: 'Enter the directory path:',
        placeholder: './videos',
        validate: (value) => {
          if (!value) return 'Please provide a directory path';
          if (!existsSync(value)) return 'Directory does not exist';
          return undefined;
        },
      });
      if (isCancel(dirInput)) handleCancel();
      searchDir = String(dirInput);
    } else {
      searchDir = String(dirChoice);
    }
  }

  const searchPath = searchDir ? path.join(searchDir, '*.mp4') : '*.mp4';
  log.info(`Searching for videos in: ${searchPath}`);
  const files = (await glob(searchPath)).sort();
  log.success(`Found ${files.length} video files`);
  return files;
}

async function getFfmpegOptions(): Promise<FfmpegOptions> {
  const customizeOptions = await confirm({
    message: 'Would you like to customize the conversion settings?',
    initialValue: false,
  });

  if (isCancel(customizeOptions)) handleCancel();

  if (!customizeOptions) {
    return {
      codec: 'libvpx-vp9',
      crf: 30,
      bitrate: 0,
      scale: '1280:-1',
      audio: false,
    };
  }

  note(
    pc.white(
      unwrap`FFmpeg Configuration:

Codec (libvpx-vp9): VP9 codec, best for web video
CRF (30): Constant Rate Factor - lower = better quality (15-35)
Bitrate (0): 0 means variable bitrate based on CRF
Scale (1280:-1): Output width, -1 maintains aspect ratio
Audio: Whether to include audio in output`,
    ),
  );

  const options = await group(
    {
      codec: () =>
        text({
          message: 'Video codec:',
          placeholder: 'libvpx-vp9',
          initialValue: 'libvpx-vp9',
          validate: (value) => (!value ? 'Please provide a codec' : undefined),
        }),
      crf: () =>
        text({
          message: 'Constant Rate Factor (15-35, lower = better quality):',
          placeholder: '30',
          initialValue: '30',
          validate: (value) => {
            const num = parseInt(value);
            if (isNaN(num) || num < 15 || num > 35)
              return 'Please enter a number between 15 and 35';
            return undefined;
          },
        }),
      bitrate: () =>
        text({
          message: 'Bitrate (0 for variable):',
          placeholder: '0',
          initialValue: '0',
          validate: (value) => {
            const num = parseInt(value);
            if (isNaN(num) || num < 0)
              return 'Please enter a non-negative number';
            return undefined;
          },
        }),
      scale: () =>
        text({
          message: 'Scale (width:height, use -1 to maintain aspect):',
          placeholder: '1280:-1',
          initialValue: '1280:-1',
          validate: (value) =>
            !value ? 'Please provide scaling dimensions' : undefined,
        }),
      audio: () =>
        confirm({
          message: 'Include audio in output?',
          initialValue: false,
        }),
    },
    {
      onCancel: () => {
        handleCancel();
      },
    },
  );

  return {
    codec: String(options.codec),
    crf: parseInt(String(options.crf)),
    bitrate: parseInt(String(options.bitrate)),
    scale: String(options.scale),
    audio: Boolean(options.audio),
  };
}

function formatFileSize(bytes: number): string {
  const units = ['B', 'KB', 'MB', 'GB'];
  let size = bytes;
  let unitIndex = 0;

  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex++;
  }

  return `${size.toFixed(2)} ${units[unitIndex]}`;
}

async function convertVideo(
  file: string,
  options: FfmpegOptions,
): Promise<ConversionResult> {
  const inputDir = path.dirname(file);
  const outputFile = path.join(inputDir, path.basename(file, '.mp4') + '.webm');
  log.info(`Converting ${file} to ${outputFile}`);

  const inputSize = statSync(file).size;
  const inputSizeFormatted = formatFileSize(inputSize);

  const audioFlag = options.audio ? '' : '-an';

  const command = $`ffmpeg -y -i ${file} -c:v ${options.codec} -crf ${options.crf} -b:v ${options.bitrate} ${audioFlag} -vf scale=${options.scale} ${outputFile}`;
  if (!argv.verbose) {
    await command.quiet();
  } else {
    await command;
  }

  const outputSize = statSync(outputFile).size;
  const outputSizeFormatted = formatFileSize(outputSize);
  const compressionPercent = (
    ((inputSize - outputSize) / inputSize) *
    100
  ).toFixed(1);

  return {
    inputSize: inputSizeFormatted,
    outputSize: outputSizeFormatted,
    compressionPercent,
    inputFile: file,
    outputFile,
  };
}

/**
 * Take the tagged string and remove indentation and word-wrapping.
 *
 * @package util
 */
function unwrap(strings: TemplateStringsArray, ...expressions: any[]): string {
  return dedent(strings, ...expressions);
}

interface FileSelection {
  filePath: string;
  fileName: string;
  exists: boolean;
}

function createFileSelection(filePath: string): FileSelection {
  const fileName = path.basename(filePath);
  const webmPath = path.join(
    path.dirname(filePath),
    path.basename(filePath, '.mp4') + '.webm',
  );
  return {
    filePath,
    fileName,
    exists: existsSync(webmPath),
  };
}

async function selectFilesToConvert(videoFiles: string[]): Promise<string[]> {
  const fileSelections = videoFiles
    .map(createFileSelection)
    .sort((a, b) => a.fileName.localeCompare(b.fileName));

  const selectedFiles = await multiselect<string>({
    message: 'Select videos to convert to WebM',
    options: fileSelections.map((file) => ({
      value: file.filePath,
      label: file.fileName,
    })),
    required: true,
    initialValues: fileSelections.map((f) => f.filePath),
  });

  if (isCancel(selectedFiles)) handleCancel();
  return selectedFiles as string[];
}

async function handleExistingFiles(
  filesToConvert: string[],
): Promise<string[]> {
  const fileSelections = filesToConvert
    .map(createFileSelection)
    .sort((a, b) => a.fileName.localeCompare(b.fileName));
  const existingFiles = fileSelections.filter((f) => f.exists);

  if (existingFiles.length === 0) {
    return filesToConvert;
  }

  const filesToOverride = await multiselect({
    message:
      'The following files already exist. Select which ones to override:',
    options: existingFiles.map((file) => ({
      value: file.filePath,
      label: file.fileName.replace('.mp4', '.webm'),
    })),
    required: false,
    initialValues: existingFiles.map((f) => f.filePath),
  });

  if (isCancel(filesToOverride)) handleCancel();

  // Create a set of files to keep (non-existing + selected to override)
  const filesToOverrideSet = new Set(filesToOverride as string[]);
  return filesToConvert
    .sort((a, b) => path.basename(a).localeCompare(path.basename(b)))
    .filter((file) => {
      const webmPath = path.join(
        path.dirname(file),
        path.basename(file, '.mp4') + '.webm',
      );
      return !existsSync(webmPath) || filesToOverrideSet.has(file);
    });
}

async function processBatch(
  files: string[],
  ffmpegOptions: FfmpegOptions,
  batchSize: number,
) {
  const sortedFiles = [...files].sort((a, b) =>
    path.basename(a).localeCompare(path.basename(b)),
  );

  for (let i = 0; i < sortedFiles.length; i += batchSize) {
    const batch = sortedFiles.slice(i, i + batchSize);
    const s = spinner();

    // Get batch file names
    const batchFileNames = batch.map((file) => path.basename(file));
    const totalFiles = sortedFiles.length;

    s.start(
      `Converting batch ${pc.dim(`[${batchFileNames.join(', ')}]`)} of ${totalFiles} files...`,
    );

    try {
      const results = await Promise.all(
        batch.map(async (file) => {
          try {
            const result = await convertVideo(file, ffmpegOptions);
            const successResult: ConversionTaskResult = {
              success: true,
              result,
            };
            return successResult;
          } catch (error) {
            const errorResult: ConversionTaskResult = {
              success: false,
              file,
              error: error instanceof Error ? error.message : String(error),
            };
            return errorResult;
          }
        }),
      );

      // Stop spinner and show results
      s.stop(`Completed batch ${pc.dim(`[${batchFileNames.join(', ')}]`)}`);

      // Display results in sorted order
      results
        .sort((a, b) => {
          const aName = a.success
            ? path.basename(a.result.inputFile)
            : path.basename(a.file);
          const bName = b.success
            ? path.basename(b.result.inputFile)
            : path.basename(b.file);
          return aName.localeCompare(bName);
        })
        .forEach((result) => {
          if (result.success) {
            const {
              inputFile,
              outputFile,
              inputSize,
              outputSize,
              compressionPercent,
            } = result.result;
            log.message(
              `${pc.bold(path.basename(inputFile))} (${inputSize}) ${pc.dim(
                '→',
              )} ${pc.bold(path.basename(outputFile))} (${outputSize}) ${pc.green(
                `(${compressionPercent}% compression)`,
              )}`,
              { symbol: pc.green('✔') },
            );
          } else {
            log.error(
              `${pc.red('✖')} ${path.basename(result.file)}: ${result.error}`,
            );
          }
        });
    } catch (error) {
      s.stop(
        `${pc.red('Batch conversion failed')}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
}

async function convertVideos() {
  intro(pc.blue('🎥 Video Converter'));

  note(
    pc.white(
      unwrap`This utility will help you convert MP4 videos to optimized WebM format.

✨ We'll automatically:
1. Search for MP4 files in the selected directory
2. Convert selected videos to WebM using VP9 codec
3. (Optionally) Configure FFmpeg settings

💡 Use --verbose for detailed logs
💡 Use --dir to specify a different directory

Press Ctrl+C to cancel.`,
    ),
  );

  log.info('Starting video conversion...');

  // Get ffmpeg options
  const ffmpegOptions = await getFfmpegOptions();

  // Find video files
  const videoFiles = await findVideoFiles(argv.dir);
  if (videoFiles.length === 0) {
    outro(pc.yellow('No MP4 files found'));
    process.exit(0);
  }

  // Let user select which files to convert
  let filesToConvert = await selectFilesToConvert(videoFiles);
  if (filesToConvert.length === 0) {
    outro(pc.yellow('No files selected'));
    process.exit(0);
  }

  // Handle existing files
  filesToConvert = await handleExistingFiles(filesToConvert);
  if (filesToConvert.length === 0) {
    outro(pc.yellow('No files to convert after skipping existing files'));
    process.exit(0);
  }

  // Get optimal batch size based on CPU cores
  const cpuCount = os.cpus().length;
  const defaultBatchSize = Math.min(cpuCount, filesToConvert.length);

  // Ask for batch size
  const batchSizeInput = await text({
    message: `How many files to process in parallel? ${pc.dim(`(${cpuCount} CPUs available, using more may slow down your system)`)}`,
    placeholder: String(defaultBatchSize),
    initialValue: String(defaultBatchSize),
    validate: (value) => {
      const num = parseInt(value);
      if (isNaN(num) || num < 1) return 'Please enter a number greater than 0';
      if (num > filesToConvert.length)
        return `Maximum batch size is ${filesToConvert.length}`;
      if (num > cpuCount)
        return pc.yellow(
          `Warning: Using more processes than available CPUs (${cpuCount}) may slow down your system`,
        );
      return undefined;
    },
  });

  if (isCancel(batchSizeInput)) handleCancel();
  const batchSize = parseInt(String(batchSizeInput));

  // Process files in batches
  await processBatch(filesToConvert, ffmpegOptions, batchSize);

  outro(pc.green('✨ Conversion complete!'));
}

convertVideos().catch((err) => {
  log.error(`${err}`);
  process.exit(1);
});
