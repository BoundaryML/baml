package baml

import (
	"runtime"
	"sync/atomic"
	"unsafe"

	"github.com/boundaryml/baml/engine/language_client_go/baml_go"
)

// ---- public API ------------------------------------------------------------
const collectorType = "collector"
const usageType = "usage"

type Collector interface {
	// Usage gathers the Usage object but keeps the underlying C collector alive
	// until the Usage is done.
	Usage() (*Usage, error)
	id() int64
}

func NewCollector() Collector {
	cPtr, err := baml_go.CallCollectorFunction(nil, collectorType, "new")
	if err != nil {
		panic(err)
	}
	return newCollector(cPtr)
}

// ---- implementation --------------------------------------------------------

type collector struct {
	c    unsafe.Pointer
	refs int32  // reference count (wrapper itself == 1)
	once uint32 // ensure destroy runs exactly once
}

func (c *collector) id() int64 {
	return int64(uintptr(c.c))
}

func newCollector(cPtr unsafe.Pointer) *collector {
	col := &collector{c: cPtr, refs: 1}

	// Tell the GC “this Go object represents ≈nativeBytes of memory”.
	// With < Go-1.22 you’d skip the 3rd arg and instead keep a []byte of the
	// same size inside the struct to bias the GC.
	runtime.SetFinalizer(col, (*collector).finalize)

	return col
}

func (c *collector) finalize() {
	// Drop our own ref; if zero we own the final destruction.
	if atomic.AddInt32(&c.refs, -1) == 0 {
		c.destroy()
	}
}

func (c *collector) destroy() {
	// Make absolutely sure we never double-free from racing finalizers.
	if atomic.CompareAndSwapUint32(&c.once, 0, 1) {
		baml_go.CallCollectorFunction(c.c, collectorType, "destroy")
	}
}

func (c *collector) Usage() (*Usage, error) {
	uPtr, err := baml_go.CallCollectorFunction(c.c, collectorType, "usage")
	if err != nil {
		return nil, err
	}

	atomic.AddInt32(&c.refs, 1) // Usage holds an extra reference
	u := &Usage{c: uPtr, parent: c}
	runtime.SetFinalizer(u, (*Usage).finalize)
	return u, nil
}

// ---- Usage wrapper ---------------------------------------------------------

type Usage struct {
	c      unsafe.Pointer
	parent *collector
}

func (u *Usage) finalize() {
	// Destroy the C-side Usage first …
	baml_go.CallCollectorFunction(u.c, usageType, "destroy")
	// … then drop the extra reference on the collector.
	u.parent.finalize()
}

func (u *Usage) InputTokens() (int, error) {
	ptr, err := baml_go.CallCollectorFunction(u.c, usageType, "input_tokens")
	if err != nil {
		return 0, err
	}
	return int(uintptr(ptr)), nil // NB: safer to have the C layer return C.int
}

func (u *Usage) OutputTokens() (int, error) {
	ptr, err := baml_go.CallCollectorFunction(u.c, usageType, "output_tokens")
	if err != nil {
		return 0, err
	}
	return int(uintptr(ptr)), nil
}
