#!/usr/bin/env node

import inquirer from 'inquirer';
import chalk from 'chalk';
import { execa } from 'execa';
import { existsSync, mkdirSync } from 'fs';
import { join, resolve } from 'path';
import fs from 'fs';

function showHelp() {
  console.log(chalk.bold.magenta('\nCreate BAML App 🦙'));
  console.log('\nUsage:');
  console.log('  npx create-baml-app [project-name] [options]');
  console.log('\nOptions:');
  console.log('  -h, --help              Show this help message');
  console.log('  --use-current-dir, -.   Use current directory instead of creating a new one');
  console.log('\nExamples:');
  console.log('  npx create-baml-app my-baml-app');
  console.log('  npx create-baml-app my-baml-app --use-current-dir');
  console.log('  npx create-baml-app my-baml-app -.');
  console.log('\nSupported Package Managers:');
  console.log('  - Python: pip, poetry, uv');
  console.log('  - TypeScript: npm, pnpm, yarn, deno, bun');
  console.log('  - Ruby: bundle');
  process.exit(0);
}

async function runCommand(command, args, description, options = {}) {
  console.log(chalk.cyan(`\nRunning: ${description}...`));
  try {
    const { stdout } = await execa(command, args, { stdio: 'inherit', ...options });
    console.log(chalk.green('✓ Success!'));
    return stdout;
  } catch (error) {
    if (error.code === 'ENOENT') {
      console.error(chalk.red(`Error: Command '${command}' not found. Please make sure it is installed and available in your PATH.`));
    } else {
      console.error(chalk.red(`Error: ${error.message}`));
    }
    process.exit(1);
  }
}

// Check if a command is available without exiting if not
async function checkCommandAvailable(command) {
  try {
    await execa(command, ['--version'], { stdio: 'ignore' });
    return true;
  } catch (error) {
    return false;
  }
}

async function main() {
  console.log(chalk.bold.magenta('\nWelcome to create-baml-app! 🦙'));
  
  // Handle command line arguments
  const args = process.argv.slice(2);
  
  // Show help if requested
  if (args.includes('-h') || args.includes('--help')) {
    showHelp();
  }

  // Get project name from arguments or prompt
  let projectName = args[0];
  if (!projectName) {
    const response = await inquirer.prompt([
      {
        type: 'input',
        name: 'projectName',
        message: 'What is your project named? (Use "." to create in current directory)',
        default: 'my-app',
        validate: (input) => {
          if (input === '.') return true;
          if (!/^[a-zA-Z0-9-_]+$/.test(input)) {
            return 'Project name can only contain letters, numbers, dashes and underscores';
          }
          return true;
        }
      }
    ]);
    projectName = response.projectName;
  }

  // Determine target directory
  const useCurrentDir = projectName === '.' || args.includes('--use-current-dir') || args.includes('-.');
  const targetDir = useCurrentDir ? process.cwd() : resolve(projectName);

  console.log(chalk.yellow("\nLet's set up your BAML project...\n"));

  // Step 1: Select Language
  const { language } = await inquirer.prompt([
    {
      type: 'list',
      name: 'language',
      message: chalk.blue('Which language would you like to use?'),
      choices: ['Python', 'TypeScript', 'Ruby'],
    },
  ]);

  // Step 2: Select Package Manager based on Language
  let packageManagerChoices;
  switch (language) {
    case 'Python':
      packageManagerChoices = ['pip', 'poetry', 'uv'];
      break;
    case 'TypeScript':
      packageManagerChoices = ['npm', 'pnpm', 'yarn', 'deno', 'bun'];
      break;
    case 'Ruby':
      packageManagerChoices = ['bundle'];
      break;
  }

  const { packageManager } = await inquirer.prompt([
    {
      type: 'list',
      name: 'packageManager',
      message: chalk.blue(`Which package manager would you like to use for ${language}?`),
      choices: packageManagerChoices,
    },
  ]);

  // Check if the package manager is available before creating directories
  let isPackageManagerAvailable = await checkCommandAvailable(packageManager);
  
  // For npm/pnpm/yarn, we might need to check node instead
  if (!isPackageManagerAvailable && ['npm', 'pnpm', 'yarn'].includes(packageManager)) {
    isPackageManagerAvailable = await checkCommandAvailable('node');
  }
  
  if (!isPackageManagerAvailable) {
    console.log(chalk.red(`\nError: ${packageManager} is not installed or not available in PATH`));
    console.log(chalk.yellow(`Please install ${packageManager} first.`));
    
    let installLink = '';
    switch (packageManager) {
      case 'pip':
        installLink = 'https://pip.pypa.io/en/stable/installation/';
        break;
      case 'poetry':
        installLink = 'https://python-poetry.org/docs/#installation';
        break;
      case 'uv':
        installLink = 'https://github.com/astral-sh/uv#installation';
        break;
      case 'npm':
        installLink = 'https://docs.npmjs.com/downloading-and-installing-node-js-and-npm';
        break;
      case 'pnpm':
        installLink = 'https://pnpm.io/installation';
        break;
      case 'yarn':
        installLink = 'https://classic.yarnpkg.com/en/docs/install';
        break;
      case 'deno':
        installLink = 'https://deno.land/#installation';
        break;
      case 'bundle':
        installLink = 'https://bundler.io/#getting-started';
        break;
      case 'bun':
        installLink = 'https://bun.sh/docs/installation';
        break;
    }
    
    console.log(chalk.gray(`${installLink}\n`));
    process.exit(1);
  }
  
  // Only check if directory is empty if using current directory
  if (useCurrentDir && existsSync(targetDir) && fs.readdirSync(targetDir).length > 0) {
    console.error(chalk.red('\nError: Current directory is not empty. Please use a new directory or empty the current one.'));
    process.exit(1);
  }

  // Create directory if not using current directory
  if (!useCurrentDir) {
    if (existsSync(targetDir)) {
      console.error(chalk.red(`\nError: Directory ${projectName} already exists. Please choose a different name or use --use-current-dir to use an existing directory.`));
      process.exit(1);
    }
    
    console.log(chalk.cyan(`\nCreating a new BAML app in ${chalk.green(targetDir)}`));
    try {
      mkdirSync(targetDir, { recursive: true });
    } catch (error) {
      console.error(chalk.red(`Error creating directory: ${error.message}`));
      process.exit(1);
    }
  }

  // Change to target directory
  try {
    process.chdir(targetDir);
  } catch (error) {
    console.error(chalk.red(`Error changing to directory: ${error.message}`));
    process.exit(1);
  }

  // Step 3: Execute Commands
  console.log(chalk.magenta(`\nSetting up your ${language} BAML project with ${packageManager}...\n`));

  if (language === 'Python') {
    if (packageManager === 'pip') {
      // Initialize requirements.txt if it doesn't exist
      if (!existsSync('requirements.txt')) {
        console.log(chalk.yellow('\nInitializing new Python project with pip...'));
        await runCommand('touch', ['requirements.txt'], 'Creating requirements.txt');
      }
      
      // Continue with the rest of the installation
      await runCommand('pip', ['install', 'baml-py'], 'Installing baml-py');
      await runCommand('baml-cli', ['init'], 'Initializing BAML');
      await runCommand('baml-cli', ['generate'], 'Generating BAML files');
    } else if (packageManager === 'poetry') {
      // Initialize poetry project if pyproject.toml doesn't exist
      if (!existsSync('pyproject.toml')) {
        console.log(chalk.yellow('\nInitializing new Python project with Poetry...'));
        await runCommand('poetry', ['init', '--name=baml-project', '--description=A BAML project', '--author=', '--python=^3.8', '--no-interaction'], 'Creating Poetry project');
      }
      await runCommand('poetry', ['add', 'baml-py'], 'Adding baml-py');
      await runCommand('poetry', ['run', 'baml-cli', 'init'], 'Initializing BAML');
      await runCommand('poetry', ['run', 'baml-cli', 'generate'], 'Generating BAML files');
    } else if (packageManager === 'uv') {
      // Initialize pyproject.toml properly with uv init if it doesn't exist
      if (!existsSync('pyproject.toml')) {
        console.log(chalk.yellow('\nInitializing new Python project with uv...'));
        await runCommand('uv', ['init'], 'Initializing uv project');
      }
      await runCommand('uv', ['add', 'baml-py'], 'Adding baml-py');
      await runCommand('uv', ['run', 'baml-cli', 'init'], 'Initializing BAML');
      await runCommand('uv', ['run', 'baml-cli', 'generate'], 'Generating BAML files');
    }
  } else if (language === 'TypeScript') {
    // Install TypeScript first based on package manager (except for Deno)
    if (packageManager !== 'deno' && packageManager !== 'bun') {
      switch (packageManager) {
        case 'npm':
          await runCommand('npm', ['install', 'typescript', '--save-dev'], 'Installing TypeScript');
          break;
        case 'pnpm':
          await runCommand('pnpm', ['add', 'typescript', '-D'], 'Installing TypeScript');
          break;
        case 'yarn':
          await runCommand('yarn', ['add', 'typescript', '--dev'], 'Installing TypeScript');
          break;
      }
    } else if (packageManager === 'bun') {
      await runCommand('bun', ['add', 'typescript', '--dev'], 'Installing TypeScript');
    }

    // Create tsconfig.json if it doesn't exist
    if (!existsSync('tsconfig.json')) {
      try {
        switch (packageManager) {
          case 'npm':
            await runCommand('npx', ['tsc', '--init'], 'Creating tsconfig.json');
            break;
          case 'pnpm':
            await runCommand('pnpm', ['exec', 'tsc', '--init'], 'Creating tsconfig.json');
            break;
          case 'yarn':
            await runCommand('yarn', ['tsc', '--init'], 'Creating tsconfig.json');
            break;
          case 'bun':
            await runCommand('bun', ['x', 'tsc', '--init'], 'Creating tsconfig.json');
            break;
          case 'deno':
            // For Deno, we'll create a basic tsconfig.json directly
            console.log(chalk.cyan('\nCreating tsconfig.json for Deno...'));
            const denoConfig = {
              "compilerOptions": {
                "target": "ES2022",
                "lib": ["ES2022", "DOM"],
                "module": "ES2022",
                "moduleResolution": "node",
                "esModuleInterop": true,
                "strict": true,
                "skipLibCheck": true
              }
            };
            fs.writeFileSync('tsconfig.json', JSON.stringify(denoConfig, null, 2));
            console.log(chalk.green('✓ Success!'));
            break;
        }
      } catch (error) {
        console.error(chalk.red(`Error creating tsconfig.json: ${error.message}`));
        process.exit(1);
      }
    }

    if (packageManager === 'npm') {
      await runCommand('npm', ['install', '@boundaryml/baml'], 'Installing @boundaryml/baml');
      await runCommand('npx', ['baml-cli', 'init'], 'Initializing BAML');
      await runCommand('npx', ['baml-cli', 'generate'], 'Generating BAML files');
    } else if (packageManager === 'pnpm') {
      await runCommand('pnpm', ['add', '@boundaryml/baml'], 'Adding @boundaryml/baml');
      await runCommand('pnpm', ['exec', 'baml-cli', 'init'], 'Initializing BAML');
      await runCommand('pnpm', ['exec', 'baml-cli', 'generate'], 'Generating BAML files');
    } else if (packageManager === 'yarn') {
      await runCommand('yarn', ['add', '@boundaryml/baml'], 'Adding @boundaryml/baml');
      await runCommand('yarn', ['baml-cli', 'init'], 'Initializing BAML');
      await runCommand('yarn', ['baml-cli', 'generate'], 'Generating BAML files');
    } else if (packageManager === 'deno') {
      // For Deno, create deno.json if it doesn't exist
      if (!existsSync('deno.json')) {
        console.log(chalk.yellow('\nInitializing new Deno project...'));
        await runCommand('deno', ['init'], 'Creating deno.json');
      }
      await runCommand('deno', ['install', 'npm:@boundaryml/baml'], 'Installing @boundaryml/baml');
      await runCommand('deno', ['run', '-A', 'npm:@boundaryml/baml/baml-cli', 'init'], 'Initializing BAML');
      await runCommand('deno', ['run', '--unstable-sloppy-imports', '-A', 'npm:@boundaryml/baml/baml-cli', 'generate'], 'Generating BAML files');
      console.log(chalk.yellow('\nNote: As per BAML docs, for Deno in VSCode, add this to your config:'));
      console.log(chalk.gray('{\n  "deno.unstable": ["sloppy-imports"]\n}'));
    } else if (packageManager === 'bun') {
      // For Bun, create package.json if it doesn't exist
      if (!existsSync('package.json')) {
        console.log(chalk.yellow('\nInitializing new Bun project...'));
        await runCommand('bun', ['init', '-y'], 'Creating package.json', { cwd: targetDir });
      }
      await runCommand('bun', ['install', '@boundaryml/baml'], 'Installing @boundaryml/baml', { cwd: targetDir });
      await runCommand('bun', ['x', 'baml-cli', 'init'], 'Initializing BAML', { cwd: targetDir });
      await runCommand('bun', ['x', 'baml-cli', 'generate'], 'Generating BAML files', { cwd: targetDir });
    }
  } else if (language === 'Ruby') {
    // Initialize Gemfile if it doesn't exist
    if (!existsSync('Gemfile')) {
      console.log(chalk.yellow('\nInitializing new Ruby project...'));
      await runCommand('bundle', ['init'], 'Creating Gemfile');
    }
    await runCommand('bundle', ['add', 'baml', 'sorbet-runtime'], 'Adding baml and sorbet-runtime');
    await runCommand('bundle', ['exec', 'baml-cli', 'init'], 'Initializing BAML');
    await runCommand('bundle', ['exec', 'baml-cli', 'generate'], 'Generating BAML files');
  }

  console.log(chalk.bold.green('\n🚀 All done! Your BAML project is ready.'));
  console.log(chalk.cyan('Next steps: Check your project directory and start coding!'));
}

main().catch((error) => {
  console.error(chalk.red('An unexpected error occurred:', error.message));
  process.exit(1);
});
