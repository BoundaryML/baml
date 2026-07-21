package baml_go

import (
	"bytes"
	"context"
	"crypto/sha256"
	"fmt"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"sync/atomic"
	"testing"
	"time"
)

func TestResolveRuntimeDownloadsCachesAndWorksOffline(t *testing.T) {
	payload := []byte("native runtime fixture")
	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		requests.Add(1)
		_, _ = writer.Write(payload)
	}))

	artifact := testRuntimeArtifact(server.URL, payload)
	config := RuntimeConfig{CacheDir: t.TempDir(), Artifact: &artifact}
	path, version, err := resolveRuntime(context.Background(), config)
	if err != nil {
		t.Fatal(err)
	}
	if version != artifact.Version {
		t.Fatalf("got version %q, want %q", version, artifact.Version)
	}
	if requests.Load() != 1 {
		t.Fatalf("got %d requests, want 1", requests.Load())
	}
	contents, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if string(contents) != string(payload) {
		t.Fatalf("got cached contents %q, want %q", contents, payload)
	}
	if err := verifyFile(path, artifact.SHA256); err != nil {
		t.Fatalf("cached artifact failed verification: %v", err)
	}

	server.Close()
	config.DisableDownload = true
	offlinePath, _, err := resolveRuntime(context.Background(), config)
	if err != nil {
		t.Fatalf("resolve cached artifact offline: %v", err)
	}
	if offlinePath != path {
		t.Fatalf("got offline path %q, want %q", offlinePath, path)
	}
}

func TestResolveRuntimeUsesSharedCFFIManifestArtifact(t *testing.T) {
	payload := []byte("shared CFFI runtime fixture")
	digest := sha256.Sum256(payload)
	var requests atomic.Int32
	var server *httptest.Server
	server = httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		requests.Add(1)
		switch request.URL.Path {
		case "/version/0.14.1.json":
			_, _ = fmt.Fprintf(writer, `{"schema":1,"version":"0.14.1","cffi":{"aarch64-apple-darwin":{"url":%q,"sha256":"%x"}}}`, server.URL+"/libbaml_cffi-aarch64-apple-darwin.dylib", digest)
		case "/libbaml_cffi-aarch64-apple-darwin.dylib":
			_, _ = writer.Write(payload)
		default:
			http.NotFound(writer, request)
		}
	}))

	config := RuntimeConfig{
		CacheDir:        t.TempDir(),
		Version:         "0.14.1",
		Target:          "aarch64-apple-darwin",
		ManifestBaseURL: server.URL,
	}
	path, version, err := resolveRuntime(context.Background(), config)
	if err != nil {
		t.Fatal(err)
	}
	if version != "0.14.1" {
		t.Fatalf("got version %q, want 0.14.1", version)
	}
	contents, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(contents, payload) {
		t.Fatalf("downloaded runtime is %q, want %q", contents, payload)
	}
	if requests.Load() != 2 {
		t.Fatalf("got %d requests, want manifest plus artifact", requests.Load())
	}

	server.Close()
	config.DisableDownload = true
	if _, _, err := resolveRuntime(context.Background(), config); err != nil {
		t.Fatalf("offline manifest and runtime cache failed: %v", err)
	}
}

func TestDisableDownloadPreventsManifestAndArtifactRequests(t *testing.T) {
	var requests atomic.Int32
	client := &http.Client{Transport: roundTripFunc(func(*http.Request) (*http.Response, error) {
		requests.Add(1)
		return nil, fmt.Errorf("network must not be reached")
	})}
	config := RuntimeConfig{
		CacheDir:        t.TempDir(),
		Version:         "0.14.1",
		Target:          "aarch64-apple-darwin",
		ManifestBaseURL: "https://example.invalid",
		DisableDownload: true,
		HTTPClient:      client,
	}
	if _, _, err := resolveRuntime(context.Background(), config); err == nil || !strings.Contains(err.Error(), "downloads are disabled") {
		t.Fatalf("got error %v, want disabled-download error", err)
	}
	if requests.Load() != 0 {
		t.Fatalf("offline resolution performed %d HTTP requests", requests.Load())
	}

	artifact := testRuntimeArtifact("https://example.invalid/runtime", []byte("runtime"))
	config.Artifact = &artifact
	if _, _, err := resolveRuntime(context.Background(), config); err == nil {
		t.Fatal("offline missing artifact unexpectedly resolved")
	}
	if requests.Load() != 0 {
		t.Fatalf("offline artifact resolution performed %d HTTP requests", requests.Load())
	}
}

func TestManifestVersionCannotEscapeCacheDirectory(t *testing.T) {
	cache := t.TempDir()
	escapePath := filepath.Join(cache, "escape.json")
	if err := os.WriteFile(escapePath, []byte(`{"schema":1}`), 0o644); err != nil {
		t.Fatal(err)
	}
	config := RuntimeConfig{
		CacheDir:        filepath.Join(cache, "manifests-root"),
		Version:         "../../escape",
		Target:          "aarch64-apple-darwin",
		ManifestBaseURL: "https://example.invalid",
		DisableDownload: true,
	}
	if _, err := resolveManifestArtifact(context.Background(), config); err == nil || !strings.Contains(err.Error(), "safe path segment") {
		t.Fatalf("got error %v, want unsafe-version diagnostic", err)
	}
	contents, err := os.ReadFile(escapePath)
	if err != nil {
		t.Fatal(err)
	}
	if string(contents) != `{"schema":1}` {
		t.Fatalf("outside-cache file was modified: %q", contents)
	}
}

func TestDisableDownloadEnvironmentCannotBeOverridden(t *testing.T) {
	t.Setenv("BAML_DISABLE_DOWNLOAD", "true")
	t.Setenv("BAML_RUNTIME_TARGET", "aarch64-apple-darwin")

	configuredRuntime.Lock()
	previous := configuredRuntime.value
	configuredRuntime.value = &RuntimeConfig{DisableDownload: false}
	configuredRuntime.Unlock()
	t.Cleanup(func() {
		configuredRuntime.Lock()
		configuredRuntime.value = previous
		configuredRuntime.Unlock()
	})

	config, err := currentRuntimeConfig()
	if err != nil {
		t.Fatal(err)
	}
	if !config.DisableDownload {
		t.Fatal("BAML_DISABLE_DOWNLOAD=true was overridden by programmatic configuration")
	}
}

func TestProcessLockSerializesInstallers(t *testing.T) {
	path := filepath.Join(t.TempDir(), "runtime.lock")
	first, err := acquireProcessLock(context.Background(), path)
	if err != nil {
		t.Fatal(err)
	}
	acquired := make(chan *processLock, 1)
	errors := make(chan error, 1)
	go func() {
		lock, err := acquireProcessLock(context.Background(), path)
		if err != nil {
			errors <- err
			return
		}
		acquired <- lock
	}()
	select {
	case lock := <-acquired:
		lock.release()
		t.Fatal("second installer acquired a held process lock")
	case err := <-errors:
		t.Fatal(err)
	case <-time.After(100 * time.Millisecond):
	}
	first.release()
	select {
	case lock := <-acquired:
		lock.release()
	case err := <-errors:
		t.Fatal(err)
	case <-time.After(2 * time.Second):
		t.Fatal("second installer did not acquire released process lock")
	}
}

func TestResolveRuntimeRejectsBadDownloadChecksum(t *testing.T) {
	payload := []byte("tampered runtime")
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		_, _ = writer.Write(payload)
	}))
	defer server.Close()

	artifact := testRuntimeArtifact(server.URL, []byte("expected runtime"))
	cache := t.TempDir()
	if _, _, err := resolveRuntime(context.Background(), RuntimeConfig{CacheDir: cache, Artifact: &artifact}); err == nil {
		t.Fatal("checksum mismatch unexpectedly succeeded")
	}
	destination := filepath.Join(cache, artifact.Version, "abi-v1", artifact.Target, artifact.Filename)
	if _, err := os.Stat(destination); !os.IsNotExist(err) {
		t.Fatalf("bad download was installed at %s", destination)
	}
}

func TestResolveRuntimeConcurrentFirstUseDownloadsOnce(t *testing.T) {
	payload := []byte("one immutable runtime")
	var requests atomic.Int32
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		requests.Add(1)
		_, _ = writer.Write(payload)
	}))
	defer server.Close()

	artifact := testRuntimeArtifact(server.URL, payload)
	config := RuntimeConfig{CacheDir: t.TempDir(), Artifact: &artifact}
	const goroutines = 24
	var wait sync.WaitGroup
	errors := make(chan error, goroutines)
	paths := make(chan string, goroutines)
	for range goroutines {
		wait.Add(1)
		go func() {
			defer wait.Done()
			path, _, err := resolveRuntime(context.Background(), config)
			if err != nil {
				errors <- err
				return
			}
			paths <- path
		}()
	}
	wait.Wait()
	close(errors)
	close(paths)
	for err := range errors {
		t.Error(err)
	}
	var expectedPath string
	for path := range paths {
		if expectedPath == "" {
			expectedPath = path
		} else if path != expectedPath {
			t.Errorf("got path %q, want %q", path, expectedPath)
		}
	}
	if requests.Load() != 1 {
		t.Fatalf("got %d downloads, want 1", requests.Load())
	}
}

func TestResolveRuntimeRepairsCorruptCacheWhenOnline(t *testing.T) {
	payload := []byte("correct runtime")
	server := httptest.NewServer(http.HandlerFunc(func(writer http.ResponseWriter, request *http.Request) {
		_, _ = writer.Write(payload)
	}))
	defer server.Close()

	artifact := testRuntimeArtifact(server.URL, payload)
	cache := t.TempDir()
	destination := filepath.Join(cache, artifact.Version, "abi-v1", artifact.Target, artifact.Filename)
	if err := os.MkdirAll(filepath.Dir(destination), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(destination, []byte("corrupt"), 0o755); err != nil {
		t.Fatal(err)
	}
	path, _, err := resolveRuntime(context.Background(), RuntimeConfig{CacheDir: cache, Artifact: &artifact})
	if err != nil {
		t.Fatal(err)
	}
	if err := verifyFile(path, artifact.SHA256); err != nil {
		t.Fatal(err)
	}
}

func TestResolveRuntimeExplicitPathDoesNotNeedArtifact(t *testing.T) {
	path := filepath.Join(t.TempDir(), defaultRuntimeFilename())
	if err := os.WriteFile(path, []byte("fixture"), 0o755); err != nil {
		t.Fatal(err)
	}
	resolved, expectedVersion, err := resolveRuntime(context.Background(), RuntimeConfig{LibraryPath: path, DisableDownload: true})
	if err != nil {
		t.Fatal(err)
	}
	if resolved != path || expectedVersion != "" {
		t.Fatalf("got (%q, %q), want (%q, empty)", resolved, expectedVersion, path)
	}
}

func TestValidateArtifactRejectsCacheTraversal(t *testing.T) {
	artifact := testRuntimeArtifact("https://example.invalid/runtime", []byte("runtime"))
	for name, mutate := range map[string]func(*RuntimeArtifact){
		"version":  func(value *RuntimeArtifact) { value.Version = "../escape" },
		"target":   func(value *RuntimeArtifact) { value.Target = "../../escape" },
		"filename": func(value *RuntimeArtifact) { value.Filename = "../escape.dylib" },
	} {
		t.Run(name, func(t *testing.T) {
			candidate := artifact
			mutate(&candidate)
			if err := validateArtifact(candidate); err == nil {
				t.Fatal("unsafe artifact unexpectedly validated")
			}
		})
	}
}

func testRuntimeArtifact(url string, payload []byte) RuntimeArtifact {
	digest := sha256.Sum256(payload)
	return RuntimeArtifact{
		Version:  "0.14.1-test",
		Target:   defaultRuntimeTarget(),
		URL:      url,
		SHA256:   fmt.Sprintf("%x", digest),
		Filename: defaultRuntimeFilename(),
	}
}

type roundTripFunc func(*http.Request) (*http.Response, error)

func (function roundTripFunc) RoundTrip(request *http.Request) (*http.Response, error) {
	return function(request)
}
