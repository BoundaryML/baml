package main

type Status int

const (
	Todo Status = iota
	Doing
	Review
	Done
)

type Task struct {
	id     int
	status Status
}

func advance(s Status, n int) Status {
	if s == Todo {
		return Doing
	}
	if s == Doing {
		if n%3 == 0 {
			return Review
		}
		return Doing
	}
	if s == Review {
		if n%5 == 0 {
			return Doing
		}
		return Done
	}
	return Todo
}

func Main() int {
	tasks := []*Task{}
	for i := 0; i < 1000; i++ {
		tasks = append(tasks, &Task{id: i, status: Todo})
	}
	done := 0
	for round := 0; round < 40; round++ {
		for _, t := range tasks {
			t.status = advance(t.status, t.id+round)
			if t.status == Done {
				done += t.id%7 + 1
			}
		}
	}
	return done
}
