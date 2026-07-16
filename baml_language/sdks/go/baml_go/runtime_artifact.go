package baml_go

import (
	"bytes"
	"compress/gzip"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net/http"
	"os"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"
)

const (
	runtimeABIVersion              = 1
	maximumManifestSizeBytes       = int64(4 << 20)
	maximumRuntimeArchiveSizeBytes = int64(256 << 20)
	maximumRuntimeSizeBytes        = int64(1 << 30)
	defaultRuntimeManifestBaseURL  = "https://pkg.boundaryml.com/manifest/v1"
)

// RuntimeArtifact identifies one immutable native BAML runtime artifact.
type RuntimeArtifact struct {
	Version       string
	Target        string
	URL           string
	SHA256        string
	ArchiveSHA256 string
	Format        string
	Filename      string
}

// RuntimeConfig controls how the native BAML runtime is resolved. Configure
// it before the first generated BAML call. Zero values inherit environment
// defaults.
type RuntimeConfig struct {
	LibraryPath     string
	CacheDir        string
	Version         string
	Target          string
	ManifestBaseURL string
	Artifact        *RuntimeArtifact
	DisableDownload bool
	HTTPClient      *http.Client
}

var configuredRuntime struct {
	sync.RWMutex
	value *RuntimeConfig
}

var artifactLocks sync.Map

// ConfigureRuntime sets process-wide native-runtime resolution. It may be
// called repeatedly until a runtime has been loaded.
func ConfigureRuntime(config RuntimeConfig) error {
	nativeRuntime.Lock()
	defer nativeRuntime.Unlock()
	if nativeRuntime.loaded {
		return errors.New("configure BAML runtime: native runtime is already loaded")
	}
	copy := cloneRuntimeConfig(config)
	configuredRuntime.Lock()
	configuredRuntime.value = &copy
	configuredRuntime.Unlock()
	return nil
}

func cloneRuntimeConfig(config RuntimeConfig) RuntimeConfig {
	copy := config
	if config.Artifact != nil {
		artifact := *config.Artifact
		copy.Artifact = &artifact
	}
	return copy
}

func currentRuntimeConfig() (RuntimeConfig, error) {
	configuredRuntime.RLock()
	configured := configuredRuntime.value
	var config RuntimeConfig
	if configured != nil {
		config = cloneRuntimeConfig(*configured)
	}
	configuredRuntime.RUnlock()

	if config.Artifact != nil {
		if config.Version == "" {
			config.Version = config.Artifact.Version
		}
		if config.Target == "" {
			config.Target = config.Artifact.Target
		}
	}
	if config.LibraryPath == "" {
		config.LibraryPath = os.Getenv("BAML_RUNTIME_PATH")
	}
	if config.CacheDir == "" {
		config.CacheDir = os.Getenv("BAML_CACHE_DIR")
	}
	if config.Version == "" {
		config.Version = os.Getenv("BAML_RUNTIME_VERSION")
	}
	if config.Version == "" {
		config.Version = requiredRuntimeVersion()
	}
	if config.Target == "" {
		config.Target = os.Getenv("BAML_RUNTIME_TARGET")
	}
	if config.Target == "" {
		var err error
		config.Target, err = nativeRuntimeTarget()
		if err != nil {
			return RuntimeConfig{}, err
		}
	}
	if config.ManifestBaseURL == "" {
		config.ManifestBaseURL = os.Getenv("BAML_RUNTIME_MANIFEST_BASE_URL")
	}
	if config.ManifestBaseURL == "" {
		config.ManifestBaseURL = defaultRuntimeManifestBaseURL
	}
	config.ManifestBaseURL = strings.TrimRight(config.ManifestBaseURL, "/")
	if disabled, present := os.LookupEnv("BAML_DISABLE_DOWNLOAD"); present {
		value, err := strconv.ParseBool(disabled)
		if err != nil {
			return RuntimeConfig{}, fmt.Errorf("parse BAML_DISABLE_DOWNLOAD: %w", err)
		}
		config.DisableDownload = config.DisableDownload || value
	}
	if config.Artifact == nil && os.Getenv("BAML_RUNTIME_URL") != "" {
		filename := os.Getenv("BAML_RUNTIME_FILENAME")
		if filename == "" {
			filename = defaultRuntimeFilename()
		}
		config.Artifact = &RuntimeArtifact{
			Version:       config.Version,
			Target:        config.Target,
			URL:           os.Getenv("BAML_RUNTIME_URL"),
			SHA256:        os.Getenv("BAML_RUNTIME_SHA256"),
			ArchiveSHA256: os.Getenv("BAML_RUNTIME_ARCHIVE_SHA256"),
			Format:        os.Getenv("BAML_RUNTIME_FORMAT"),
			Filename:      filename,
		}
	}
	return config, nil
}

func resolveRuntime(ctx context.Context, config RuntimeConfig) (string, string, error) {
	if config.LibraryPath != "" {
		path, err := filepath.Abs(config.LibraryPath)
		if err != nil {
			return "", "", fmt.Errorf("resolve BAML_RUNTIME_PATH: %w", err)
		}
		if err := requireFile(path); err != nil {
			return "", "", fmt.Errorf("resolve BAML_RUNTIME_PATH: %w", err)
		}
		return path, "", nil
	}

	if config.CacheDir == "" {
		var err error
		config.CacheDir, err = defaultRuntimeCacheDir()
		if err != nil {
			return "", "", err
		}
	}
	if config.Artifact != nil {
		if config.Version == "" {
			config.Version = config.Artifact.Version
		}
		if config.Target == "" {
			config.Target = config.Artifact.Target
		}
	}
	if config.Version == "" {
		config.Version = requiredRuntimeVersion()
	}
	if config.Target == "" {
		var err error
		config.Target, err = nativeRuntimeTarget()
		if err != nil {
			return "", "", err
		}
	}
	if config.ManifestBaseURL == "" {
		config.ManifestBaseURL = defaultRuntimeManifestBaseURL
	}
	if config.Artifact == nil {
		artifact, err := resolveManifestArtifact(ctx, config)
		if err != nil {
			return "", "", err
		}
		config.Artifact = &artifact
	}
	artifact := *config.Artifact
	if err := validateArtifact(artifact); err != nil {
		return "", "", err
	}
	path := filepath.Join(config.CacheDir, artifact.Version, fmt.Sprintf("abi-v%d", runtimeABIVersion), artifact.Target, artifact.Filename)

	lockValue, _ := artifactLocks.LoadOrStore(path, &sync.Mutex{})
	lock := lockValue.(*sync.Mutex)
	lock.Lock()
	defer lock.Unlock()

	if err := verifyFile(path, artifact.SHA256); err == nil {
		return path, artifact.Version, nil
	} else if config.DisableDownload {
		return "", "", fmt.Errorf("BAML runtime %s is unavailable offline at %s: %w", artifact.Version, path, err)
	}
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return "", "", fmt.Errorf("create BAML runtime cache: %w", err)
	}
	processLock, err := acquireProcessLock(ctx, path+".lock")
	if err != nil {
		return "", "", err
	}
	defer processLock.release()
	if err := verifyFile(path, artifact.SHA256); err == nil {
		return path, artifact.Version, nil
	}
	if err := downloadArtifact(ctx, config.HTTPClient, artifact, path); err != nil {
		return "", "", err
	}
	return path, artifact.Version, nil
}

type releaseManifest struct {
	Schema  uint32                            `json:"schema"`
	Version string                            `json:"version"`
	CFFI    map[string]releaseRuntimeArtifact `json:"cffi"`
}

type releaseRuntimeArtifact struct {
	URL    string `json:"url"`
	SHA256 string `json:"sha256"`
}

func resolveManifestArtifact(ctx context.Context, config RuntimeConfig) (RuntimeArtifact, error) {
	manifestPath := filepath.Join(config.CacheDir, "manifests", config.Version+".json")
	contents, err := os.ReadFile(manifestPath)
	if err != nil && !os.IsNotExist(err) {
		return RuntimeArtifact{}, fmt.Errorf("read cached BAML runtime manifest: %w", err)
	}
	if os.IsNotExist(err) {
		if config.DisableDownload {
			return RuntimeArtifact{}, fmt.Errorf("BAML runtime downloads are disabled and manifest %s is not cached", config.Version)
		}
		if err := os.MkdirAll(filepath.Dir(manifestPath), 0o755); err != nil {
			return RuntimeArtifact{}, fmt.Errorf("create BAML manifest cache: %w", err)
		}
		lock, err := acquireProcessLock(ctx, manifestPath+".lock")
		if err != nil {
			return RuntimeArtifact{}, err
		}
		defer lock.release()
		contents, err = os.ReadFile(manifestPath)
		if os.IsNotExist(err) {
			manifestURL := strings.TrimRight(config.ManifestBaseURL, "/") + "/version/" + config.Version + ".json"
			contents, err = downloadBytes(ctx, config.HTTPClient, manifestURL, maximumManifestSizeBytes)
			if err != nil {
				return RuntimeArtifact{}, fmt.Errorf("download BAML runtime manifest: %w", err)
			}
			if err := writeFileAtomically(manifestPath, contents, 0o644); err != nil {
				return RuntimeArtifact{}, fmt.Errorf("cache BAML runtime manifest: %w", err)
			}
		} else if err != nil {
			return RuntimeArtifact{}, fmt.Errorf("read cached BAML runtime manifest: %w", err)
		}
	}

	var manifest releaseManifest
	if err := json.Unmarshal(contents, &manifest); err != nil {
		return RuntimeArtifact{}, fmt.Errorf("decode BAML runtime manifest: %w", err)
	}
	if manifest.Schema != 1 {
		return RuntimeArtifact{}, fmt.Errorf("unsupported BAML runtime manifest schema %d", manifest.Schema)
	}
	if manifest.Version != config.Version {
		return RuntimeArtifact{}, fmt.Errorf("BAML runtime manifest version mismatch: got %q, want %q", manifest.Version, config.Version)
	}
	entry, ok := manifest.CFFI[config.Target]
	if !ok {
		return RuntimeArtifact{}, fmt.Errorf("BAML runtime manifest %s has no CFFI artifact for %s", config.Version, config.Target)
	}
	artifact := RuntimeArtifact{
		Version:  config.Version,
		Target:   config.Target,
		URL:      entry.URL,
		SHA256:   entry.SHA256,
		Filename: runtimeFilenameForTarget(config.Target),
	}
	if err := validateArtifact(artifact); err != nil {
		return RuntimeArtifact{}, fmt.Errorf("invalid BAML runtime manifest entry for %s: %w", config.Target, err)
	}
	return artifact, nil
}

func validateArtifact(artifact RuntimeArtifact) error {
	for name, value := range map[string]string{
		"version":  artifact.Version,
		"target":   artifact.Target,
		"URL":      artifact.URL,
		"filename": artifact.Filename,
	} {
		if strings.TrimSpace(value) == "" {
			return fmt.Errorf("BAML runtime artifact has an empty %s", name)
		}
	}
	if !safePathSegment(artifact.Version) {
		return fmt.Errorf("BAML runtime artifact version %q is not a safe path segment", artifact.Version)
	}
	if !safePathSegment(artifact.Target) {
		return fmt.Errorf("BAML runtime artifact target %q is not a safe path segment", artifact.Target)
	}
	if artifact.Filename != filepath.Base(artifact.Filename) {
		return fmt.Errorf("BAML runtime artifact filename %q is not a base filename", artifact.Filename)
	}
	digest, err := hex.DecodeString(artifact.SHA256)
	if err != nil || len(digest) != sha256.Size {
		return fmt.Errorf("BAML runtime artifact SHA256 must be exactly 64 hexadecimal characters")
	}
	if artifact.Format != "" && artifact.Format != "gzip" {
		return fmt.Errorf("BAML runtime artifact format %q is unsupported", artifact.Format)
	}
	if artifact.Format == "gzip" {
		digest, err := hex.DecodeString(artifact.ArchiveSHA256)
		if err != nil || len(digest) != sha256.Size {
			return fmt.Errorf("compressed BAML runtime artifact archive SHA256 must be exactly 64 hexadecimal characters")
		}
	}
	return nil
}

func safePathSegment(value string) bool {
	return value != "." && value != ".." && filepath.Base(value) == value
}

func defaultRuntimeCacheDir() (string, error) {
	if home := strings.TrimSpace(os.Getenv("BAML_HOME")); home != "" {
		return filepath.Join(home, "runtimes"), nil
	}
	home, err := os.UserHomeDir()
	if err != nil {
		return "", fmt.Errorf("locate home directory for BAML runtime cache: %w", err)
	}
	return filepath.Join(home, ".baml", "runtimes"), nil
}

func requireFile(path string) error {
	info, err := os.Stat(path)
	if err != nil {
		return err
	}
	if !info.Mode().IsRegular() {
		return fmt.Errorf("%s is not a regular file", path)
	}
	return nil
}

func verifyFile(path, expectedSHA256 string) error {
	file, err := os.Open(path)
	if err != nil {
		return err
	}
	defer file.Close()
	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return fmt.Errorf("hash %s: %w", path, err)
	}
	actual := hex.EncodeToString(hash.Sum(nil))
	if !strings.EqualFold(actual, expectedSHA256) {
		return fmt.Errorf("SHA-256 mismatch: got %s, want %s", actual, strings.ToLower(expectedSHA256))
	}
	return nil
}

func downloadArtifact(ctx context.Context, client *http.Client, artifact RuntimeArtifact, destination string) error {
	if err := os.MkdirAll(filepath.Dir(destination), 0o755); err != nil {
		return fmt.Errorf("create BAML runtime cache: %w", err)
	}
	if artifact.Format == "gzip" {
		archive, err := downloadBytes(ctx, client, artifact.URL, maximumRuntimeArchiveSizeBytes)
		if err != nil {
			return fmt.Errorf("download BAML runtime %s: %w", artifact.Version, err)
		}
		actualArchiveSHA256 := fmt.Sprintf("%x", sha256.Sum256(archive))
		if !strings.EqualFold(actualArchiveSHA256, artifact.ArchiveSHA256) {
			return fmt.Errorf("download BAML runtime %s: archive SHA-256 mismatch: got %s, want %s", artifact.Version, actualArchiveSHA256, strings.ToLower(artifact.ArchiveSHA256))
		}
		reader, err := gzip.NewReader(bytes.NewReader(archive))
		if err != nil {
			return fmt.Errorf("decompress BAML runtime %s: %w", artifact.Version, err)
		}
		defer reader.Close()
		return installRuntime(reader, artifact, destination)
	}
	response, err := openDownload(ctx, client, artifact.URL)
	if err != nil {
		return fmt.Errorf("download BAML runtime %s: %w", artifact.Version, err)
	}
	defer response.Body.Close()
	return installRuntime(response.Body, artifact, destination)
}

func installRuntime(source io.Reader, artifact RuntimeArtifact, destination string) error {
	temporary, err := os.CreateTemp(filepath.Dir(destination), ".baml-runtime-*")
	if err != nil {
		return fmt.Errorf("create temporary BAML runtime: %w", err)
	}
	temporaryPath := temporary.Name()
	defer os.Remove(temporaryPath)

	hash := sha256.New()
	written, copyErr := io.Copy(io.MultiWriter(temporary, hash), io.LimitReader(source, maximumRuntimeSizeBytes+1))
	if copyErr != nil {
		temporary.Close()
		return fmt.Errorf("write BAML runtime download: %w", copyErr)
	}
	if written > maximumRuntimeSizeBytes {
		temporary.Close()
		return fmt.Errorf("BAML runtime download exceeds %d bytes", maximumRuntimeSizeBytes)
	}
	actualSHA256 := hex.EncodeToString(hash.Sum(nil))
	if !strings.EqualFold(actualSHA256, artifact.SHA256) {
		temporary.Close()
		return fmt.Errorf("download BAML runtime %s: SHA-256 mismatch: got %s, want %s", artifact.Version, actualSHA256, strings.ToLower(artifact.SHA256))
	}
	if err := temporary.Sync(); err != nil {
		temporary.Close()
		return fmt.Errorf("sync BAML runtime download: %w", err)
	}
	if err := temporary.Chmod(0o755); err != nil {
		temporary.Close()
		return fmt.Errorf("set BAML runtime permissions: %w", err)
	}
	if err := temporary.Close(); err != nil {
		return fmt.Errorf("close BAML runtime download: %w", err)
	}
	if err := replaceFile(temporaryPath, destination); err != nil {
		return fmt.Errorf("install BAML runtime in cache: %w", err)
	}
	return nil
}

func openDownload(ctx context.Context, client *http.Client, url string) (*http.Response, error) {
	if client == nil {
		client = &http.Client{Timeout: 10 * time.Minute}
	}
	request, err := http.NewRequestWithContext(ctx, http.MethodGet, url, nil)
	if err != nil {
		return nil, fmt.Errorf("create request: %w", err)
	}
	response, err := client.Do(request)
	if err != nil {
		return nil, err
	}
	if response.StatusCode != http.StatusOK {
		_, _ = io.Copy(io.Discard, io.LimitReader(response.Body, 64<<10))
		response.Body.Close()
		return nil, fmt.Errorf("unexpected HTTP status %s", response.Status)
	}
	return response, nil
}

func downloadBytes(ctx context.Context, client *http.Client, url string, limit int64) ([]byte, error) {
	response, err := openDownload(ctx, client, url)
	if err != nil {
		return nil, err
	}
	defer response.Body.Close()
	contents, err := io.ReadAll(io.LimitReader(response.Body, limit+1))
	if err != nil {
		return nil, err
	}
	if int64(len(contents)) > limit {
		return nil, fmt.Errorf("download exceeds %d bytes", limit)
	}
	return contents, nil
}

func writeFileAtomically(path string, contents []byte, mode os.FileMode) error {
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		return err
	}
	temporary, err := os.CreateTemp(filepath.Dir(path), ".baml-metadata-*")
	if err != nil {
		return err
	}
	temporaryPath := temporary.Name()
	defer os.Remove(temporaryPath)
	if _, err := temporary.Write(contents); err != nil {
		temporary.Close()
		return err
	}
	if err := temporary.Sync(); err != nil {
		temporary.Close()
		return err
	}
	if err := temporary.Chmod(mode); err != nil {
		temporary.Close()
		return err
	}
	if err := temporary.Close(); err != nil {
		return err
	}
	return replaceFile(temporaryPath, path)
}

func replaceFile(source, destination string) error {
	if runtime.GOOS == "windows" {
		if err := os.Remove(destination); err != nil && !os.IsNotExist(err) {
			return err
		}
	}
	return os.Rename(source, destination)
}

type processLock struct {
	path string
}

func acquireProcessLock(ctx context.Context, path string) (*processLock, error) {
	deadline := time.NewTimer(15 * time.Minute)
	defer deadline.Stop()
	for {
		file, err := os.OpenFile(path, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o600)
		if err == nil {
			_, _ = fmt.Fprintf(file, "pid=%d created=%s\n", os.Getpid(), time.Now().UTC().Format(time.RFC3339))
			if closeErr := file.Close(); closeErr != nil {
				_ = os.Remove(path)
				return nil, closeErr
			}
			return &processLock{path: path}, nil
		}
		if !os.IsExist(err) {
			return nil, fmt.Errorf("acquire BAML runtime cache lock %s: %w", path, err)
		}
		if info, statErr := os.Stat(path); statErr == nil && time.Since(info.ModTime()) > 30*time.Minute {
			_ = os.Remove(path)
			continue
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-deadline.C:
			return nil, fmt.Errorf("timed out waiting for BAML runtime cache lock %s", path)
		case <-time.After(50 * time.Millisecond):
		}
	}
}

func (lock *processLock) release() {
	_ = os.Remove(lock.path)
}

func defaultRuntimeTarget() string {
	target, err := nativeRuntimeTarget()
	if err != nil {
		return runtime.GOOS + "-" + runtime.GOARCH
	}
	return target
}

func defaultRuntimeFilename() string {
	return runtimeFilenameForTarget(defaultRuntimeTarget())
}

func runtimeFilenameForTarget(target string) string {
	switch {
	case strings.Contains(target, "apple-darwin"):
		return "libbridge_cffi.dylib"
	case strings.HasSuffix(target, "windows-msvc"):
		return "bridge_cffi.dll"
	default:
		return "libbridge_cffi.so"
	}
}
