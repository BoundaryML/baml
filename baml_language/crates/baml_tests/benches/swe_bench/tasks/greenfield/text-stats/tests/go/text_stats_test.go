// Go grader for the text-stats task. Builds the candidate Go program
// from text_stats.go at the staging root and runs it against each
// fixture under inputs/.
package go_grader

import (
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

type expected struct {
	Bytes int `json:"bytes"`
	Chars int `json:"chars"`
	Words int `json:"words"`
	Lines int `json:"lines"`
}

var fixtures = map[string]expected{
	"empty.txt":      {Bytes: 0, Chars: 0, Words: 0, Lines: 0},
	"hello.txt":      {Bytes: 12, Chars: 12, Words: 2, Lines: 1},
	"multi_line.txt": {Bytes: 34, Chars: 34, Words: 6, Lines: 3},
	"unicode.txt":    {Bytes: 31, Chars: 24, Words: 5, Lines: 2},
}

// stagingRoot resolves to the directory that contains text_stats.go, inputs/, etc.
// `go test` runs with cwd == the package directory (tests/go/), so we walk up
// two levels.
func stagingRoot(t *testing.T) string {
	t.Helper()
	wd, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	return filepath.Clean(filepath.Join(wd, "..", ".."))
}

func buildCandidate(t *testing.T, root string) string {
	t.Helper()
	src := filepath.Join(root, "text_stats.go")
	if _, err := os.Stat(src); err != nil {
		t.Fatalf("missing candidate %s: %v", src, err)
	}
	tmp, err := os.MkdirTemp("", "text_stats_build_")
	if err != nil {
		t.Fatalf("mkdtemp: %v", err)
	}
	t.Cleanup(func() { os.RemoveAll(tmp) })
	bin := filepath.Join(tmp, "text_stats_bin")
	cmd := exec.Command("go", "build", "-o", bin, src)
	cmd.Dir = root
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("go build failed: %v\n%s", err, out)
	}
	return bin
}

func runCandidate(t *testing.T, bin, fixture string) expected {
	t.Helper()
	out, err := exec.Command(bin, fixture).Output()
	if err != nil {
		t.Fatalf("candidate failed for %s: %v\n%s", fixture, err, out)
	}
	var got expected
	if err := json.Unmarshal(out, &got); err != nil {
		t.Fatalf("invalid JSON for %s: %v\n%q", fixture, err, out)
	}
	return got
}

func TestTextStats(t *testing.T) {
	root := stagingRoot(t)
	bin := buildCandidate(t, root)
	for name, want := range fixtures {
		t.Run(name, func(t *testing.T) {
			path := filepath.Join(root, "inputs", name)
			if _, err := os.Stat(path); err != nil {
				t.Fatalf("missing fixture: %v", err)
			}
			got := runCandidate(t, bin, path)
			if got != want {
				t.Fatalf("got %s want %s", mustJSON(got), mustJSON(want))
			}
		})
	}
}

func mustJSON(v any) string {
	b, _ := json.Marshal(v)
	return fmt.Sprintf("%s", b)
}
