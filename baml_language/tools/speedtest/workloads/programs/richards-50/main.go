package main

type Packet struct {
	link *Packet
	id   int
	kind int
	a1   int
	a2   []int
}

func newPacket(link *Packet, id, kind int) *Packet {
	return &Packet{link: link, id: id, kind: kind, a1: 0, a2: []int{0, 0, 0, 0}}
}

func addTo(p *Packet, queue *Packet) *Packet {
	p.link = nil
	if queue == nil {
		return p
	}
	peek := queue
	for {
		next := peek.link
		if next == nil {
			break
		}
		peek = next
	}
	peek.link = p
	return queue
}

type Task interface {
	run(s *Scheduler, packet *Packet) *Tcb
}

type Tcb struct {
	link     *Tcb
	id       int
	priority int
	queue    *Packet
	task     Task
	state    int
}

func (t *Tcb) run(s *Scheduler) *Tcb {
	var p *Packet
	if t.state == 3 {
		q := t.queue
		if q != nil {
			p = q
			t.queue = q.link
			if t.queue == nil {
				t.state = 0
			} else {
				t.state = 1
			}
		}
	}
	return t.task.run(s, p)
}

func (t *Tcb) checkPriorityAdd(task *Tcb, p *Packet) *Tcb {
	if t.queue == nil {
		t.queue = p
		t.state = t.state | 1
		if t.priority > task.priority {
			return t
		}
	} else {
		t.queue = addTo(p, t.queue)
	}
	return task
}

func (t *Tcb) heldOrSuspended() bool {
	return (t.state&4) != 0 || t.state == 2
}

type Scheduler struct {
	queueCount int
	holdCount  int
	blocks     []*Tcb
	list       *Tcb
	current    *Tcb
	currentID  int
}

func (s *Scheduler) addTask(id, priority int, queue *Packet, task Task) *Tcb {
	state := 3
	if queue == nil {
		state = 2
	}
	t := &Tcb{link: s.list, id: id, priority: priority, queue: queue, task: task, state: state}
	s.current = t
	s.list = t
	s.blocks[id] = t
	return t
}

func (s *Scheduler) release(id int) *Tcb {
	t := s.blocks[id]
	if t == nil {
		return nil
	}
	t.state = t.state & -5
	c := s.current
	if c == nil || t.priority > c.priority {
		return t
	}
	return c
}

func (s *Scheduler) holdCurrent() *Tcb {
	s.holdCount++
	c := s.current
	if c == nil {
		return nil
	}
	c.state = c.state | 4
	return c.link
}

func (s *Scheduler) suspendCurrent() *Tcb {
	c := s.current
	if c == nil {
		return nil
	}
	c.state = c.state | 2
	return c
}

func (s *Scheduler) enqueue(p *Packet) *Tcb {
	t := s.blocks[p.id]
	if t == nil {
		return nil
	}
	c := s.current
	if c == nil {
		return nil
	}
	s.queueCount++
	p.link = nil
	p.id = s.currentID
	return t.checkPriorityAdd(c, p)
}

type IdleTask struct {
	v1    int
	count int
}

func (task *IdleTask) run(s *Scheduler, packet *Packet) *Tcb {
	task.count--
	if task.count == 0 {
		return s.holdCurrent()
	}
	if (task.v1 & 1) == 0 {
		task.v1 = task.v1 >> 1
		return s.release(4)
	}
	task.v1 = (task.v1 >> 1) ^ 53256
	return s.release(5)
}

type DeviceTask struct {
	v1 *Packet
}

func (task *DeviceTask) run(s *Scheduler, packet *Packet) *Tcb {
	if packet == nil {
		v := task.v1
		if v == nil {
			return s.suspendCurrent()
		}
		task.v1 = nil
		return s.enqueue(v)
	}
	task.v1 = packet
	return s.holdCurrent()
}

type WorkerTask struct {
	v1 int
	v2 int
}

func (task *WorkerTask) run(s *Scheduler, packet *Packet) *Tcb {
	if packet == nil {
		return s.suspendCurrent()
	}
	if task.v1 == 2 {
		task.v1 = 3
	} else {
		task.v1 = 2
	}
	packet.id = task.v1
	packet.a1 = 0
	for i := 0; i < 4; i++ {
		task.v2++
		if task.v2 > 26 {
			task.v2 = 1
		}
		packet.a2[i] = task.v2
	}
	return s.enqueue(packet)
}

type HandlerTask struct {
	v1 *Packet
	v2 *Packet
}

func (task *HandlerTask) run(s *Scheduler, packet *Packet) *Tcb {
	if packet != nil {
		if packet.kind == 1 {
			task.v1 = addTo(packet, task.v1)
		} else {
			task.v2 = addTo(packet, task.v2)
		}
	}
	w := task.v1
	if w != nil {
		count := w.a1
		if count < 4 {
			d := task.v2
			if d != nil {
				task.v2 = d.link
				d.a1 = w.a2[count]
				w.a1 = count + 1
				return s.enqueue(d)
			}
		} else {
			task.v1 = w.link
			return s.enqueue(w)
		}
	}
	return s.suspendCurrent()
}

func richards() int {
	s := &Scheduler{blocks: []*Tcb{nil, nil, nil, nil, nil, nil}}
	idle := s.addTask(0, 0, nil, &IdleTask{v1: 1, count: 1000})
	idle.state = 0
	q := newPacket(nil, 1, 1)
	q = newPacket(q, 1, 1)
	s.addTask(1, 1000, q, &WorkerTask{v1: 2, v2: 0})
	q = newPacket(nil, 4, 0)
	q = newPacket(q, 4, 0)
	q = newPacket(q, 4, 0)
	s.addTask(2, 2000, q, &HandlerTask{})
	q = newPacket(nil, 5, 0)
	q = newPacket(q, 5, 0)
	q = newPacket(q, 5, 0)
	s.addTask(3, 3000, q, &HandlerTask{})
	s.addTask(4, 4000, nil, &DeviceTask{})
	s.addTask(5, 5000, nil, &DeviceTask{})
	s.current = s.list
	for {
		c := s.current
		if c == nil {
			break
		}
		if c.heldOrSuspended() {
			s.current = c.link
		} else {
			s.currentID = c.id
			s.current = c.run(s)
		}
	}
	return s.queueCount*10000 + s.holdCount
}

func Main() int {
	total := 0
	for i := 0; i < 50; i++ {
		total += richards()
	}
	return total
}
