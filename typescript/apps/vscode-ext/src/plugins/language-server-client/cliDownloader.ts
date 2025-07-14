// Re-export all public functions from the refactored modules
export {
  type CliVersion,
  downloadedCliPath,
  checkIfDownloadedCliExists,
  getReleaseArchitecture,
  getReleasePlatform,
  downloadCli,
  resolveCliPath,
} from './cli-downloader';
