package main

// Iterator is the pull protocol: Next returns the next item, or false when done.
type Iterator[T any] interface {
	Next() (T, bool)
}

type sliceIter[T any] struct {
	values []T
	pos    int
}

func (s *sliceIter[T]) Next() (T, bool) {
	if s.pos >= len(s.values) {
		var zero T
		return zero, false
	}
	v := s.values[s.pos]
	s.pos++
	return v, true
}

func iterOf[T any](values []T) Iterator[T] {
	return &sliceIter[T]{values: values}
}

type filterIter[T any] struct {
	inner Iterator[T]
	pred  func(T) bool
}

func (f *filterIter[T]) Next() (T, bool) {
	for {
		v, ok := f.inner.Next()
		if !ok || f.pred(v) {
			return v, ok
		}
	}
}

func filter[T any](inner Iterator[T], pred func(T) bool) Iterator[T] {
	return &filterIter[T]{inner: inner, pred: pred}
}

type mapIter[T, U any] struct {
	inner Iterator[T]
	f     func(T) U
}

func (m *mapIter[T, U]) Next() (U, bool) {
	v, ok := m.inner.Next()
	if !ok {
		var zero U
		return zero, false
	}
	return m.f(v), true
}

func mapped[T, U any](inner Iterator[T], f func(T) U) Iterator[U] {
	return &mapIter[T, U]{inner: inner, f: f}
}

func reduce[T, A any](it Iterator[T], f func(A, T) A, init A) A {
	acc := init
	for {
		v, ok := it.Next()
		if !ok {
			return acc
		}
		acc = f(acc, v)
	}
}

// Window implements the iterator protocol itself.
type Window struct {
	values []int
	pos    int
	end    int
}

func (w *Window) Iter() Iterator[int] {
	return w
}

func (w *Window) Next() (int, bool) {
	if w.pos >= w.end {
		return 0, false
	}
	v := w.values[w.pos]
	w.pos++
	return v, true
}

func Main() int {
	xs := []int{}
	seed := 5
	for i := 0; i < 2000; i++ {
		seed = seed * 16807 % 2147483647
		xs = append(xs, seed%1000)
	}
	acc := 0
	for round := 0; round < 10; round++ {
		skip := round % 3
		acc += reduce(
			mapped(
				filter(iterOf(xs), func(x int) bool { return x%3 != skip }),
				func(x int) int { return x*2 + skip }),
			func(a, x int) int { return a + x }, 0)
		w := &Window{values: xs, pos: round * 100, end: round*100 + 1000}
		it := w.Iter()
		for v, ok := it.Next(); ok; v, ok = it.Next() {
			acc += v % 17
		}
	}
	return acc
}
