package baml_go

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"math"
	"net"
	"net/http"
	"os"
	"path/filepath"
	"reflect"
	"runtime"
	"strings"
	"sync"
	"time"
	"unsafe"
)

/*
#cgo CFLAGS: -I${SRCDIR}
#cgo CFLAGS: -O3 -g
#include <baml_cffi_wrapper.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
*/
import "C"

const (
	VERSION            = "0.218.1"
	githubRepo         = "boundaryml/baml"
	bamlCacheDirEnvVar = "BAML_CACHE_DIR"
	bamlLibraryPathEnv = "BAML_LIBRARY_PATH"
	bamlDisableDlEnv   = "BAML_LIBRARY_DISABLE_DOWNLOAD"
)

// uninstrumentedHTTPClient creates an HTTP client for use during init().
// Uses reflection to set DD__tracer_internal flag if it exists (added by Orchestrion).
func uninstrumentedHTTPClient() *http.Client {
	transport := &http.Transport{
		Proxy: http.ProxyFromEnvironment,
		DialContext: (&net.Dialer{
			Timeout:   30 * time.Second,
			KeepAlive: 30 * time.Second,
		}).DialContext,
		ForceAttemptHTTP2:     true,
		MaxIdleConns:          10,
		IdleConnTimeout:       90 * time.Second,
		TLSHandshakeTimeout:   10 * time.Second,
		ExpectContinueTimeout: 1 * time.Second,
	}

	// Try to set DD__tracer_internal field using reflection
	// This field only exists when Orchestrion transforms the code
	setOrchestrionInternalFlag(transport)

	return &http.Client{
		Transport: transport,
		Timeout:   5 * time.Minute,
	}
}

// setOrchestrionInternalFlag tries to set DD__tracer_internal=true using reflection.
// This field is added by Orchestrion's code transformation.
func setOrchestrionInternalFlag(transport *http.Transport) {
	// Use reflection to set the field if it exists
	val := reflect.ValueOf(transport).Elem()
	field := val.FieldByName("DD__tracer_internal")
	if field.IsValid() && field.CanSet() && field.Kind() == reflect.Bool {
		field.SetBool(true)
	}
}

var (
	ErrLoadLibrary          = errors.New("baml: failed loading shared library")
	ErrNotSupportedPlatform = errors.New("baml: platform not supported (only Linux and MacOS amd64/arm64)")
	ErrDownloadFailed       = errors.New("baml: failed to download shared library")
	ErrCacheDir             = errors.New("baml: failed to determine or create cache directory")
	ErrChecksumMismatch     = errors.New("baml: downloaded library checksum mismatch")
	ErrVersionMismatch      = errors.New("baml: library version mismatch")
	ErrInitialization       = errors.New("baml: initialization failed")
)

var (
	bamlSharedLibraryPath = ""
	initErr               error
	initOnce              sync.Once
	bamlLibHandle         unsafe.Pointer
	logger                *slog.Logger
)

func SetSharedLibraryPath(path string) {
	if bamlLibHandle != nil {
		logger.Warn("SetSharedLibraryPath called after BAML library was initialized. Path ignored.", "path", path)
		return
	}
	bamlSharedLibraryPath = path
}

func init() {
	initSlog() // Initialize the logger first
	initOnce.Do(func() {
		initErr = initializeBaml()
		if initErr != nil {
			panic(initErr)
		}
	})
}

func GetInitError() error {
	return initErr
}

func initializeBaml() error {
	if !isSupportedPlatform() {
		err := fmt.Errorf("%w: OS=%s Arch=%s", ErrNotSupportedPlatform, runtime.GOOS, runtime.GOARCH)
		return err
	}

	err := findOrDownloadLibrary()
	if err != nil {
		return fmt.Errorf("%w: %w", ErrInitialization, err)
	}

	if bamlSharedLibraryPath == "" {
		err := fmt.Errorf("%w: library path discovery finished without error but path is empty", ErrInitialization)
		return err
	}

	if _, err := os.Stat(bamlSharedLibraryPath); err != nil {
		errMsg := fmt.Errorf("%w: failed to stat library file %s: %w", ErrLoadLibrary, bamlSharedLibraryPath, err)
		if errors.Is(err, os.ErrNotExist) {
			errMsg = fmt.Errorf("%w: determined path %s does not exist", ErrLoadLibrary, bamlSharedLibraryPath)
		}
		return errMsg
	}

	// Platform-specific initialization
	if err := platformInit(); err != nil {
		return fmt.Errorf("%w: %w", ErrInitialization, err)
	}

	logger.Debug("Loading BAML library", "path", bamlSharedLibraryPath)
	handle, err := loadLibrary(bamlSharedLibraryPath)
	if err != nil {
		// Enhanced error messages for common issues
		errStr := err.Error()
		if strings.Contains(errStr, "wrong architecture") ||
			strings.Contains(errStr, "wrong ELF class") ||
			strings.Contains(errStr, "is not a valid Win32 application") {
			err = fmt.Errorf("%w (possible architecture mismatch)", err)
		}
		return fmt.Errorf("%w: %w", ErrLoadLibrary, err)
	}
	bamlLibHandle = handle

	// Register all functions
	lib := library{handle: bamlLibHandle}
	if err := lib.registerFunctions(); err != nil {
		closeLibrary(bamlLibHandle)
		bamlLibHandle = nil
		return fmt.Errorf("%w: %w", ErrLoadLibrary, err)
	}

	goVersionStr := BamlVersion()

	if goVersionStr != VERSION {
		closeLibrary(bamlLibHandle)
		bamlLibHandle = nil
		err := fmt.Errorf("%w: Go package expects %s, but loaded library %s reports %s",
			ErrVersionMismatch, VERSION, bamlSharedLibraryPath, goVersionStr)
		return err
	}

	logger.Info(fmt.Sprintf("BAML (v%s) loaded", goVersionStr))
	logger.Debug(fmt.Sprintf("Library path: %s", bamlSharedLibraryPath))
	return nil
}

type library struct{ handle unsafe.Pointer }

func (l *library) registerFunctions() error {
	var symbolLookupErr error
	func() {
		defer func() {
			if r := recover(); r != nil {
				symbolLookupErr = fmt.Errorf("panic during symbol lookup: %v", r)
			}
		}()
		l.registerFn("version")
		l.registerFn("create_baml_runtime")
		l.registerFn("destroy_baml_runtime")
		l.registerFn("invoke_runtime_cli")
		l.registerFn("register_callbacks")
		l.registerFn("call_function_from_c")
		l.registerFn("call_function_stream_from_c")
		l.registerFn("call_function_parse_from_c")
		l.registerFn("build_request_from_c")
		l.registerFn("cancel_function_call")
		l.registerFn("call_object_constructor")
		l.registerFn("call_object_method")
		l.registerFn("free_buffer")
	}()
	return symbolLookupErr
}

func (l *library) registerFn(fnName string) error {
	fnPtr, err := getSymbol(l.handle, fnName)
	if err != nil {
		return err
	}

	switch fnName {
	case "version":
		C.SetVersionFn(fnPtr)
	case "create_baml_runtime":
		C.SetCreateBamlRuntimeFn(fnPtr)
	case "destroy_baml_runtime":
		C.SetDestroyBamlRuntimeFn(fnPtr)
	case "invoke_runtime_cli":
		C.SetInvokeRuntimeCliFn(fnPtr)
	case "register_callbacks":
		C.SetRegisterCallbacksFn(fnPtr)
	case "call_function_from_c":
		C.SetCallFunctionFromCFn(fnPtr)
	case "call_function_stream_from_c":
		C.SetCallFunctionStreamFromCFn(fnPtr)
	case "call_function_parse_from_c":
		C.SetCallFunctionParseFromCFn(fnPtr)
	case "build_request_from_c":
		C.SetBuildRequestFromCFn(fnPtr)
	case "cancel_function_call":
		C.SetCancelFunctionCallFn(fnPtr)
	case "call_object_constructor":
		C.SetCallObjectConstructorFn(fnPtr)
	case "call_object_method":
		C.SetCallObjectMethodFunctionFn(fnPtr)
	case "free_buffer":
		C.SetFreeBufferFn(fnPtr)
	default:
		panic(fmt.Sprintf("internal error: attempted to register unknown function '%s'", fnName))
	}
	return nil
}

func findOrDownloadLibrary() error {
	if bamlSharedLibraryPath != "" {
		_, err := os.Stat(bamlSharedLibraryPath)
		if err == nil {
			logger.Debug("Using BAML library path set via SetSharedLibraryPath()", "path", bamlSharedLibraryPath)
			return nil
		}
		err = fmt.Errorf("%w: path explicitly set via SetSharedLibraryPath() %s is invalid: %w", ErrLoadLibrary, bamlSharedLibraryPath, err)
		return err
	}

	envPath := os.Getenv(bamlLibraryPathEnv)
	if envPath != "" {
		_, err := os.Stat(envPath)
		if err == nil {
			logger.Debug("Using BAML library path from environment variable", "envVar", bamlLibraryPathEnv, "path", envPath)
			bamlSharedLibraryPath = envPath
			return nil
		}
		err = fmt.Errorf("%w: path from environment variable %s (%s) is invalid: %w", ErrLoadLibrary, bamlLibraryPathEnv, envPath, err)
		return err
	}

	cacheDir, err := getCacheDir()
	if err != nil {
		return err
	}

	libFilename, err := getTargetLibFilename()
	if err != nil {
		logger.Error("Could not determine target library filename", "error", err)
		return err
	}
	cachedLibPath := filepath.Join(cacheDir, libFilename)
	logger.Debug("Checking for cached BAML library", "path", cachedLibPath)

	_, err = os.Stat(cachedLibPath)
	if err == nil {
		logger.Info("Found valid cached BAML library", "path", cachedLibPath)
		bamlSharedLibraryPath = cachedLibPath
		return nil
	}
	logger.Debug("Library not found in cache", "path", cachedLibPath)

	if strings.ToLower(os.Getenv(bamlDisableDlEnv)) == "true" {
		logger.Warn("Automatic download disabled via environment variable", "envVar", bamlDisableDlEnv)
	} else {
		logger.Debug("Attempting to download BAML library", "version", "v"+VERSION, "os", runtime.GOOS, "arch", runtime.GOARCH)
		err = downloadBamlLibrary(cacheDir, libFilename)
		if err == nil {
			bamlSharedLibraryPath = cachedLibPath
			return nil
		}
		logger.Warn(fmt.Sprintf("BAML library download failed: %s", err))
	}

	logger.Debug("Checking default system library paths")
	defaultPath := ""
	var checkedPaths []string

	if runtime.GOOS == "windows" {
		// Windows default paths
		defaultPaths := []string{
			filepath.Join(os.Getenv("ProgramFiles"), "baml", fmt.Sprintf("baml_cffi-%s.dll", VERSION)),
			filepath.Join(os.Getenv("ProgramFiles"), "baml", "baml_cffi.dll"),
			filepath.Join(os.Getenv("LOCALAPPDATA"), "baml", fmt.Sprintf("baml_cffi-%s.dll", VERSION)),
			filepath.Join(os.Getenv("LOCALAPPDATA"), "baml", "baml_cffi.dll"),
		}
		for _, path := range defaultPaths {
			if path != "" { // Skip if env var not set
				checkedPaths = append(checkedPaths, path)
				if _, err := os.Stat(path); err == nil {
					logger.Warn("Using baml library from system path", "path", path)
					bamlSharedLibraryPath = path
					return nil
				}
			}
		}
	} else {
		// Existing Unix paths
		switch runtime.GOOS {
		case "darwin":
			paths := []string{fmt.Sprintf("/usr/local/lib/libbaml-%s.dylib", VERSION), "/usr/local/lib/libbaml.dylib"}
			checkedPaths = paths
			for _, p := range paths {
				if _, e := os.Stat(p); e == nil {
					defaultPath = p
					break
				}
			}
		case "linux":
			paths := []string{fmt.Sprintf("/usr/local/lib/libbaml-%s.so", VERSION), "/usr/local/lib/libbaml.so"}
			checkedPaths = paths
			for _, p := range paths {
				if _, e := os.Stat(p); e == nil {
					defaultPath = p
					break
				}
			}
		}
	}

	if defaultPath != "" {
		logger.Warn("Found BAML library in a default system path. This might lead to version/architecture mismatches.",
			"path", defaultPath,
			"recommendation", fmt.Sprintf("Consider using cache or %s env var", bamlLibraryPathEnv))
		bamlSharedLibraryPath = defaultPath
		return nil
	}

	errorMsg := fmt.Sprintf("%s: could not find BAML library v%s for %s/%s.", ErrLoadLibrary, VERSION, runtime.GOOS, runtime.GOARCH)
	errorMsg += "\n       Resolution attempts failed:"
	errorMsg += fmt.Sprintf("\n       - Explicit path (SetSharedLibraryPath): %s", ifelse(os.Getenv(bamlLibraryPathEnv) != "", "Not set", "Checked, not found or invalid"))
	errorMsg += fmt.Sprintf("\n       - Environment var (%s): %s", bamlLibraryPathEnv, ifelse(envPath != "", envPath+" (invalid)", "Not set"))
	errorMsg += fmt.Sprintf("\n       - Cache path: %s (not found)", cachedLibPath)
	errorMsg += fmt.Sprintf("\n       - Download (%s): %s", bamlDisableDlEnv, ifelse(strings.ToLower(os.Getenv(bamlDisableDlEnv)) == "true", "Disabled", "Attempted but failed"))
	errorMsg += fmt.Sprintf("\n       - Default system paths: %v (not found)", checkedPaths)
	err = errors.New(errorMsg)
	logger.Error("Failed to find BAML library after all attempts",
		"version", VERSION,
		"os", runtime.GOOS,
		"arch", runtime.GOARCH,
		"checked_env_var", bamlLibraryPathEnv,
		"checked_cache_path", cachedLibPath,
		"download_disabled", os.Getenv(bamlDisableDlEnv),
		"checked_system_paths", checkedPaths)

	return err
}

func ifelse(condition bool, trueVal, falseVal string) string {
	if condition {
		return trueVal
	}
	return falseVal
}

func getCacheDir() (string, error) {
	cacheDir := os.Getenv(bamlCacheDirEnvVar)
	source := fmt.Sprintf("environment variable %s", bamlCacheDirEnvVar)
	if cacheDir == "" {
		userCacheDir, err := os.UserCacheDir()
		if err != nil {
			errMsg := fmt.Errorf("%w: could not determine user cache directory: %w", ErrCacheDir, err)
			logger.Error("Could not determine user cache directory", "error", err)
			return "", errMsg
		}
		// Windows: %LOCALAPPDATA%\baml\libs\{VERSION}
		// macOS: ~/Library/Caches/baml/libs/{VERSION}
		// Linux: ~/.cache/baml/libs/{VERSION}
		cacheDir = filepath.Join(userCacheDir, "baml", "libs", VERSION)
		source = "default user cache location"
	}

	logger.Debug("Using cache directory", "source", source, "path", cacheDir)

	err := os.MkdirAll(cacheDir, 0755)
	if err != nil {
		errMsg := fmt.Errorf("%w: failed to create cache directory %s: %w", ErrCacheDir, cacheDir, err)
		logger.Error("Failed to create cache directory", "path", cacheDir, "error", err)
		return "", errMsg
	}
	return cacheDir, nil
}

func getTargetLibFilename() (string, error) {
	goos, goarch := runtime.GOOS, runtime.GOARCH
	var libName, ext, targetTriple string

	switch goos {
	case "windows":
		libName = "baml_cffi" // No "lib" prefix on Windows
		ext = "dll"
		if goarch == "amd64" {
			targetTriple = "x86_64-pc-windows-msvc"
		} else if goarch == "arm64" {
			targetTriple = "aarch64-pc-windows-msvc"
		} else {
			return "", fmt.Errorf("%w: unsupported architecture %s", ErrNotSupportedPlatform, goarch)
		}
	case "linux":
		libName = "libbaml_cffi" // Keep "lib" prefix for Unix
		ext = "so"
		if isMusl() {
			if goarch == "amd64" {
				targetTriple = "x86_64-unknown-linux-musl"
			} else if goarch == "arm64" {
				targetTriple = "aarch64-unknown-linux-musl"
			} else {
				return "", fmt.Errorf("%w: unsupported architecture %s", ErrNotSupportedPlatform, goarch)
			}
		} else {
			if goarch == "amd64" {
				targetTriple = "x86_64-unknown-linux-gnu"
			} else if goarch == "arm64" {
				targetTriple = "aarch64-unknown-linux-gnu"
			} else {
				return "", fmt.Errorf("%w: unsupported architecture %s", ErrNotSupportedPlatform, goarch)
			}
		}
	case "darwin":
		libName = "libbaml_cffi" // Keep "lib" prefix for Unix
		ext = "dylib"
		if goarch == "amd64" {
			targetTriple = "x86_64-apple-darwin"
		} else if goarch == "arm64" {
			targetTriple = "aarch64-apple-darwin"
		} else {
			return "", fmt.Errorf("%w: unsupported architecture %s", ErrNotSupportedPlatform, goarch)
		}
	default:
		return "", fmt.Errorf("%w: unsupported OS %s", ErrNotSupportedPlatform, goos)
	}

	return fmt.Sprintf("%s-%s.%s", libName, targetTriple, ext), nil
}

func isMusl() bool {
	// TODO: Implement this
	return false
}

//orchestrion:ignore
func downloadBamlLibrary(destDir string, filename string) error {
	tag := VERSION
	downloadURL := fmt.Sprintf("https://github.com/%s/releases/download/%s/%s", githubRepo, tag, filename)
	checksumURL := fmt.Sprintf("https://github.com/%s/releases/download/%s/%s.sha256", githubRepo, tag, filename)
	destPath := filepath.Join(destDir, filename)

	logger.Debug("Downloading BAML library", "url", downloadURL, "destination", destPath)

	var expectedChecksum string
	logger.Debug("Checking for checksum file", "url", checksumURL)
	expectedChecksum, err := downloadChecksum(checksumURL, filename)
	if err != nil {
		logger.Warn("Could not get checksum. Download will proceed without verification.", "checksum_url", checksumURL, "error", err)
		expectedChecksum = ""
	} else {
		logger.Debug("Checksum found. Will verify after download.")
	}

	req, err := http.NewRequest("GET", downloadURL, nil)
	if err != nil {
		return fmt.Errorf("%w: failed to create download request: %w", ErrDownloadFailed, err)
	}
	req.Header.Set("User-Agent", fmt.Sprintf("baml-go/%s (%s/%s)", VERSION, runtime.GOOS, runtime.GOARCH))

	// Use uninstrumented client to avoid Orchestrion crash during init()
	//orchestrion:ignore
	httpClient := uninstrumentedHTTPClient()
	//orchestrion:ignore
	resp, err := httpClient.Do(req)
	if err != nil {
		return fmt.Errorf("%w: network error fetching %s: %w", ErrDownloadFailed, downloadURL, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		bodyBytes, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
		if resp.StatusCode == http.StatusNotFound {
			return fmt.Errorf("%w: library file not found at %s (HTTP 404). Check release tag 'v%s' and filename '%s'", ErrDownloadFailed, downloadURL, tag, filename)
		}
		return fmt.Errorf("%w: unexpected HTTP status %d fetching %s. Server response: %s", ErrDownloadFailed, resp.StatusCode, downloadURL, string(bodyBytes))
	}

	tmpFile, err := os.CreateTemp(destDir, filename+".*.tmpdl")
	if err != nil {
		return fmt.Errorf("%w: failed to create temporary download file in %s: %w", ErrDownloadFailed, destDir, err)
	}
	defer func() { tmpFile.Close(); os.Remove(tmpFile.Name()) }()

	contentLength := resp.ContentLength
	progressDesc := fmt.Sprintf("Downloading %s", filename)
	progWriter := NewProgressWriter(os.Stderr, contentLength, progressDesc)

	hasher := sha256.New()
	multiOut := io.MultiWriter(tmpFile, hasher, progWriter)

	_, err = io.Copy(multiOut, resp.Body)
	progWriter.Finish()
	if err != nil {
		return fmt.Errorf("%w: download interrupted writing to %s: %w", ErrDownloadFailed, tmpFile.Name(), err)
	}

	if err := tmpFile.Close(); err != nil {
		return fmt.Errorf("%w: failed closing temporary file %s: %w", ErrDownloadFailed, tmpFile.Name(), err)
	}

	actualChecksum := hex.EncodeToString(hasher.Sum(nil))
	if expectedChecksum != "" {
		logger.Debug("Verifying checksum")
		if actualChecksum != expectedChecksum {
			err := fmt.Errorf("%w: checksum mismatch for %s. Expected %s, got %s. File %s may be corrupt",
				ErrChecksumMismatch, filename, expectedChecksum, actualChecksum, tmpFile.Name())
			return err
		}
		logger.Info("Checksum verified successfully", "checksum_prefix", actualChecksum[:8])
	} else if contentLength > 0 {
		logger.Warn("Checksum verification skipped (checksum file not found or download failed)")
	}

	logger.Debug("Moving downloaded file to final location", "path", destPath)
	err = os.Rename(tmpFile.Name(), destPath)
	if err != nil {
		logger.Warn("Atomic rename failed, attempting copy fallback", "rename_error", err, "from", tmpFile.Name(), "to", destPath)
		if copyErr := copyFile(tmpFile.Name(), destPath); copyErr != nil {
			err := fmt.Errorf("%w: failed moving temp file %s to %s: rename failed (%w) and copy failed (%w)", ErrDownloadFailed, tmpFile.Name(), destPath, err, copyErr)
			return err
		}
		logger.Info("Copy fallback succeeded", "from", tmpFile.Name(), "to", destPath)
	}

	logger.Debug("Setting permissions for library file", "path", destPath, "mode", "0755")
	if err := os.Chmod(destPath, 0755); err != nil {
		logger.Warn("Failed to set permissions (chmod 0755)", "path", destPath, "error", err)
	}

	logger.Info("Successfully downloaded and cached BAML library", "path", destPath)
	return nil
}

//orchestrion:ignore
func downloadChecksum(checksumURL string, targetFilename string) (string, error) {
	// Use uninstrumented client to avoid Orchestrion crash during init()
	//orchestrion:ignore
	httpClient := uninstrumentedHTTPClient()
	//orchestrion:ignore
	resp, err := httpClient.Get(checksumURL)
	if err != nil {
		return "", fmt.Errorf("network error fetching checksum %s: %w", checksumURL, err)
	}
	defer resp.Body.Close()

	if resp.StatusCode == http.StatusNotFound {
		logger.Debug("Checksum file not found (404)", "url", checksumURL)
		return "", fmt.Errorf("checksum file not found (404)")
	}
	if resp.StatusCode != http.StatusOK {
		return "", fmt.Errorf("unexpected status %d fetching checksum %s", resp.StatusCode, checksumURL)
	}

	bodyBytes, err := io.ReadAll(io.LimitReader(resp.Body, 4096))
	if err != nil {
		return "", fmt.Errorf("error reading checksum body %s: %w", checksumURL, err)
	}

	lines := strings.Split(string(bodyBytes), "\n")
	for _, line := range lines {
		parts := strings.Fields(line)
		if len(parts) >= 2 {
			checksum, filenameInLine := parts[0], strings.TrimPrefix(parts[1], "*")
			if filenameInLine == targetFilename {
				if len(checksum) == 64 && isHex(checksum) {
					logger.Debug("Found matching checksum in file", "filename", targetFilename, "checksum", checksum)
					return checksum, nil
				}
				logger.Warn("Invalid checksum format found in checksum file",
					"url", checksumURL,
					"filename", targetFilename,
					"found_checksum", checksum)
				return "", fmt.Errorf("invalid checksum format '%s' for %s in %s", checksum, targetFilename, checksumURL)
			}
		}
	}

	logger.Warn("Checksum for target file not found within checksum file",
		"url", checksumURL,
		"target_filename", targetFilename)
	return "", fmt.Errorf("checksum for '%s' not found within file %s", targetFilename, checksumURL)
}

func isHex(s string) bool {
	if len(s) == 0 {
		return false
	}
	for _, r := range s {
		if !((r >= '0' && r <= '9') || (r >= 'a' && r <= 'f') || (r >= 'A' && r <= 'F')) {
			return false
		}
	}
	return true
}

func copyFile(src, dst string) (err error) {
	logger.Debug("Attempting to copy file", "source", src, "destination", dst)
	source, err := os.Open(src)
	if err != nil {
		return fmt.Errorf("failed open source %s: %w", src, err)
	}
	defer source.Close()

	destination, err := os.Create(dst)
	if err != nil {
		return fmt.Errorf("failed create destination %s: %w", dst, err)
	}
	defer func() {
		cerr := destination.Close()
		if err == nil && cerr != nil {
			err = fmt.Errorf("failed close destination %s: %w", dst, cerr)
		}
	}()

	_, err = io.Copy(destination, source)
	if err != nil {
		return fmt.Errorf("failed copying from %s to %s: %w", src, dst, err)
	}

	err = destination.Sync()
	if err != nil {
		return fmt.Errorf("failed syncing destination %s: %w", dst, err)
	}

	logger.Debug("File copied successfully", "source", src, "destination", dst)
	return nil
}

func isSupportedPlatform() bool {
	osOk := runtime.GOOS == "darwin" || runtime.GOOS == "linux" || runtime.GOOS == "windows"
	archOk := runtime.GOARCH == "amd64" || runtime.GOARCH == "arm64"
	return osOk && archOk
}

func initSlog() {
	logLevel := slog.LevelInfo

	envLevel := os.Getenv("BAML_LOG")
	switch strings.ToUpper(envLevel) {
	case "TRACE":
		logLevel = slog.LevelDebug - 1
	case "DEBUG":
		logLevel = slog.LevelDebug
	case "INFO":
		logLevel = slog.LevelInfo
	case "WARN", "WARNING":
		logLevel = slog.LevelWarn
	case "ERROR":
		logLevel = slog.LevelError
	case "OFF":
		logLevel = slog.LevelError + 1
	default:
		if envLevel != "" {
			coloredLevel := getColorForLevel("WARN")
			fmt.Fprintf(os.Stderr, "%s [BAML %s] Invalid BAML_LOG '%s'. Defaulting to %s%s%s.\n",
				time.Now().Format("2006-01-02T15:04:05.000"), coloredLevel, envLevel, getColorForLevel("INFO"), "INFO", resetColor())
			logLevel = slog.LevelInfo
		}
	}

	// Create a custom handler that formats logs according to the desired format
	handler := &customLogHandler{
		level: logLevel,
		out:   os.Stderr,
	}
	logger = slog.New(handler)
}

// customLogHandler implements slog.Handler interface
type customLogHandler struct {
	level slog.Level
	out   io.Writer
}

// Enabled implements slog.Handler.
func (h *customLogHandler) Enabled(ctx context.Context, level slog.Level) bool {
	return level >= h.level
}

// Handle implements slog.Handler.
func (h *customLogHandler) Handle(ctx context.Context, r slog.Record) error {
	levelStr := getLevelString(r.Level)
	timeStr := r.Time.Format("2006-01-02T15:04:05.000")

	msg := r.Message
	if len(msg) > 0 {
		msg = strings.ToUpper(string(msg[0])) + msg[1:]
	}

	// Format: $TIME [BAML 🐑] [$LOG_LEVEL] $MESSAGE
	levelColor := getColorForLevel(levelStr)
	coloredLevelStr := fmt.Sprintf("%s%s%s", levelColor, levelStr, resetColor())
	line := fmt.Sprintf("%s [BAML %s] %s", timeStr, coloredLevelStr, msg)

	// Add attributes if any
	if r.NumAttrs() > 0 {
		attrs := make([]string, 0, r.NumAttrs())
		r.Attrs(func(a slog.Attr) bool {
			// Skip time and level as they're already handled
			if a.Key == slog.TimeKey || a.Key == slog.LevelKey {
				return true
			}

			val := a.Value.String()
			// Check if it's a string that might need quotes
			if a.Value.Kind() == slog.KindString {
				val = fmt.Sprintf("%q", a.Value.String())
			}

			attrs = append(attrs, fmt.Sprintf("%s=%s", a.Key, val))
			return true
		})

		if len(attrs) > 0 {
			line += " " + strings.Join(attrs, " ")
		}
	}

	fmt.Fprintln(h.out, line)
	return nil
}

func resetColor() string {
	return "\033[0m"
}

func getColorForLevel(levelStr string) string {
	switch levelStr {
	case "DEBUG":
		return "\033[90m" // Gray
	case "INFO":
		return "\033[92m" // Green
	case "WARN":
		return "\033[93m" // Yellow
	case "ERROR":
		return "\033[91m" // Red
	case "TRACE":
		return "\033[94m" // Blue
	default:
		return ""
	}
}

// WithAttrs implements slog.Handler.
func (h *customLogHandler) WithAttrs(attrs []slog.Attr) slog.Handler {
	return h
}

func (h *customLogHandler) WithGroup(name string) slog.Handler {
	return h
}

// getLevelString returns the string representation of a log level
func getLevelString(level slog.Level) string {
	switch level {
	case slog.LevelDebug:
		return "DEBUG"
	case slog.LevelInfo:
		return "INFO"
	case slog.LevelWarn:
		return "WARN"
	case slog.LevelError:
		return "ERROR"
	case slog.LevelDebug - 1:
		return "TRACE"
	case slog.LevelError + 1:
		return "OFF"
	default:
		return fmt.Sprintf("L(%d)", level)
	}
}

const (
	progressUpdateInterval = 200 * time.Millisecond
	progressWidth          = 40
)

type progressWriter struct {
	dest        io.Writer
	totalSize   int64
	currentSize int64
	startTime   time.Time
	lastUpdate  time.Time
	description string
}

func NewProgressWriter(dest io.Writer, totalSize int64, description string) *progressWriter {
	return &progressWriter{
		dest:        dest,
		totalSize:   totalSize,
		startTime:   time.Now(),
		lastUpdate:  time.Now(),
		description: description,
	}
}

func (pw *progressWriter) Write(p []byte) (n int, err error) {
	n = len(p)
	pw.currentSize += int64(n)

	now := time.Now()
	if now.Sub(pw.lastUpdate) > progressUpdateInterval || pw.currentSize == pw.totalSize {
		pw.printProgress(now)
		pw.lastUpdate = now
	}
	return n, nil
}

func (pw *progressWriter) printProgress(now time.Time) {
	var percent float64
	if pw.totalSize > 0 {
		percent = float64(pw.currentSize) / float64(pw.totalSize) * 100
	}

	filledWidth := 0
	if pw.totalSize > 0 {
		filledWidth = int(math.Round((float64(pw.currentSize) / float64(pw.totalSize)) * float64(progressWidth)))
	}
	bar := strings.Repeat("=", filledWidth) + strings.Repeat(" ", progressWidth-filledWidth)

	currentSizeStr := formatBytes(pw.currentSize)
	totalSizeStr := ""
	if pw.totalSize > 0 {
		totalSizeStr = " / " + formatBytes(pw.totalSize)
	} else {
		totalSizeStr = " / ???"
	}

	elapsed := now.Sub(pw.startTime).Seconds()
	speedStr := ""
	if elapsed > 0.5 {
		speed := float64(pw.currentSize) / elapsed
		speedStr = fmt.Sprintf(" (%s/s)", formatBytes(int64(speed)))
	}

	percentStr := ""
	if pw.totalSize > 0 {
		percentStr = fmt.Sprintf(" %3.0f%%", percent)
	}

	line := fmt.Sprintf("\r%s [%s] %s%s%s%s    ",
		pw.description,
		bar,
		currentSizeStr,
		totalSizeStr,
		percentStr,
		speedStr,
	)

	fmt.Fprint(pw.dest, line)
}

func (pw *progressWriter) Finish() {
	if pw.currentSize != pw.totalSize {
		pw.printProgress(time.Now())
	}
	fmt.Fprintln(pw.dest)
}

func formatBytes(b int64) string {
	const unit = 1024
	if b < unit {
		return fmt.Sprintf("%d B", b)
	}
	div, exp := int64(unit), 0
	for n := b / unit; n >= unit; n /= unit {
		div *= unit
		exp++
	}
	return fmt.Sprintf("%.1f %ciB", float64(b)/float64(div), "KMGTPE"[exp])
}
