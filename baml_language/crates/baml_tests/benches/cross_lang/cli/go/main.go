// cross_lang_go — wrapper that dispatches to one of the workload
// functions in package crosslang. Used by the Suite A2 hyperfine
// harness so we can time Go execution as a subprocess (start +
// runtime init + execute), comparable to cross_lang_baml and
// `python3 run.py`.
//
// Usage: cross_lang_go <workload-name>
package main

import (
	"fmt"
	"os"

	crosslang "bamlbench/crosslang/workloads/go"
)

type workload struct {
	run func() any
}

var workloads = map[string]workload{
	"fib_20":               {run: func() any { return crosslang.Fib20() }},
	"fib_20_e2e":           {run: func() any { return crosslang.Fib20() }},
	"loop_500k":            {run: func() any { return crosslang.Loop500K() }},
	"string_concat_5k":     {run: func() any { return crosslang.StringConcat5K() }},
	"array_push_50k":       {run: func() any { return crosslang.ArrayPush50K() }},
	"array_iter_10k":       {run: func() any { return crosslang.ArrayIter10K() }},
	"class_create_50k":     {run: func() any { return crosslang.ClassCreate50K() }},
	"field_access_50k":     {run: func() any { return crosslang.FieldAccess50K() }},
	"call_chain_100_x_5k":  {run: func() any { return crosslang.CallChain100x5K() }},
	"nested_loop":          {run: func() any { return crosslang.NestedLoop() }},
	"mixed_ops":            {run: func() any { return crosslang.MixedOps() }},
	"closure_call_50k":     {run: func() any { return crosslang.ClosureCall50K() }},
	"hello_world":          {run: func() any { return crosslang.HelloWorld() }},
	"arithmetic":           {run: func() any { return crosslang.Arithmetic() }},
	"class_and_loop":       {run: func() any { return crosslang.ClassAndLoop() }},
	"one_hundred_functions": {run: func() any { return crosslang.OneHundredFunctions() }},
}

func main() {
	if len(os.Args) != 2 {
		fmt.Fprintln(os.Stderr, "usage: cross_lang_go <workload-name>")
		os.Exit(2)
	}
	name := os.Args[1]
	w, ok := workloads[name]
	if !ok {
		fmt.Fprintf(os.Stderr, "unknown workload: %s\n", name)
		os.Exit(2)
	}
	result := w.run()
	// Touch the result so the optimizer can't strip the call.
	_ = fmt.Sprint(result)
}
