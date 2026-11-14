// speedtest driver for Go workloads: ./workload JOBS WARMUP
//
// A workload's main.go is `package main` and defines `func Main() T`, with T an
// int, a string or a bool. One that defines `func Setup() S` and
// `func Run(S) T` has Setup called once, untimed, and Run timed instead.
// The runner builds main.go with this file and one of entry_main.go or
// entry_setup.go into one executable.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"strconv"
	"time"
)

// text renders a result the way BAML's string.from does.
func text(value any) string {
	switch v := value.(type) {
	case string:
		return v
	case bool:
		return strconv.FormatBool(v)
	case int:
		return strconv.Itoa(v)
	case int64:
		return strconv.FormatInt(v, 10)
	}
	panic(fmt.Sprintf("speedtest: unsupported result type %T", value))
}

// sink keeps every job's result alive so the compiler cannot drop the call.
var sink any

// measure verifies with one call, runs `warmup - 1` more untimed, then times
// `jobs` calls with the monotonic clock and prints one JSON report line.
func measure[T any](job func() T) {
	jobs, _ := strconv.Atoi(os.Args[1])
	warmup, _ := strconv.Atoi(os.Args[2])
	result := text(job())
	for i := 1; i < warmup; i++ {
		sink = job()
	}
	start := time.Now()
	for i := 0; i < jobs; i++ {
		sink = job()
	}
	elapsed := time.Since(start).Nanoseconds()
	out, _ := json.Marshal(map[string]any{"result": result, "elapsed_ns": elapsed, "jobs": jobs})
	fmt.Println(string(out))
}
