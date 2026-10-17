# programs::richards 50

## BAML
```baml
// Richards: Martin Richards' OS task-scheduler simulation, ported from the
// Octane / Are We Fast Yet version (BSD / MIT). Idle, worker, handler and
// device tasks pass packets through a priority scheduler. Exercises interface
// dispatch, nullable object links, field mutation and bit operations.
// 50 runs at COUNT = 1000; each must end with queue_count 2322, hold_count 928.
class Packet {
  link: Packet?
  id: int
  kind: int
  a1: int
  a2: int[]
}
function packet(link: Packet?, id: int, kind: int) -> Packet {
  Packet { link: link, id: id, kind: kind, a1: 0, a2: [0, 0, 0, 0] }
}
function add_to(p: Packet, queue: Packet?) -> Packet {
  p.link = null;
  if (queue == null) { return p; }
  let peek: Packet = queue;
  while (true) {
    let next = peek.link;
    if (next == null) { break; }
    peek = next;
  }
  peek.link = p;
  queue
}
interface Task {
  function run(self, s: Scheduler, packet: Packet?) -> Tcb? throws never
}
class Tcb {
  link: Tcb?
  id: int
  priority: int
  queue: Packet?
  task: Task
  state: int
}
function tcb_run(t: Tcb, s: Scheduler) -> Tcb? {
  let p: Packet? = null;
  if (t.state == 3) {
    let q = t.queue;
    if (q != null) {
      p = q;
      t.queue = q.link;
      if (t.queue == null) { t.state = 0; } else { t.state = 1; }
    }
  }
  t.task.run(s, p)
}
function check_priority_add(t: Tcb, task: Tcb, p: Packet) -> Tcb {
  if (t.queue == null) {
    t.queue = p;
    t.state = t.state | 1;
    if (t.priority > task.priority) { return t; }
  } else {
    t.queue = add_to(p, t.queue);
  }
  task
}
function held_or_suspended(t: Tcb) -> bool {
  (t.state & 4) != 0 || t.state == 2
}
class Scheduler {
  queue_count: int
  hold_count: int
  blocks: (Tcb?)[]
  list: Tcb?
  current: Tcb?
  current_id: int
}
function add_task(s: Scheduler, id: int, priority: int, queue: Packet?, task: Task) -> Tcb {
  let state = if (queue == null) { 2 } else { 3 };
  let t = Tcb { link: s.list, id: id, priority: priority, queue: queue, task: task, state: state };
  s.current = t;
  s.list = t;
  s.blocks[id] = t;
  t
}
function release(s: Scheduler, id: int) -> Tcb? {
  let t = s.blocks[id];
  if (t == null) { return null; }
  t.state = t.state & -5;
  let c = s.current;
  if (c == null || t.priority > c.priority) { return t; }
  c
}
function hold_current(s: Scheduler) -> Tcb? {
  s.hold_count += 1;
  let c = s.current;
  if (c == null) { return null; }
  c.state = c.state | 4;
  c.link
}
function suspend_current(s: Scheduler) -> Tcb? {
  let c = s.current;
  if (c == null) { return null; }
  c.state = c.state | 2;
  c
}
function enqueue(s: Scheduler, p: Packet) -> Tcb? {
  let t = s.blocks[p.id];
  if (t == null) { return null; }
  let c = s.current;
  if (c == null) { return null; }
  s.queue_count += 1;
  p.link = null;
  p.id = s.current_id;
  check_priority_add(t, c, p)
}
class IdleTask {
  v1: int
  count: int
  implements Task {
    function run(self, s: Scheduler, packet: Packet?) -> Tcb? {
      self.count -= 1;
      if (self.count == 0) { return hold_current(s); }
      if ((self.v1 & 1) == 0) {
        self.v1 = self.v1 >> 1;
        return release(s, 4);
      }
      self.v1 = (self.v1 >> 1) ^ 53256;
      release(s, 5)
    }
  }
}
class DeviceTask {
  v1: Packet?
  implements Task {
    function run(self, s: Scheduler, packet: Packet?) -> Tcb? {
      if (packet == null) {
        let v = self.v1;
        if (v == null) { return suspend_current(s); }
        self.v1 = null;
        return enqueue(s, v);
      }
      self.v1 = packet;
      hold_current(s)
    }
  }
}
class WorkerTask {
  v1: int
  v2: int
  implements Task {
    function run(self, s: Scheduler, packet: Packet?) -> Tcb? {
      if (packet == null) { return suspend_current(s); }
      if (self.v1 == 2) { self.v1 = 3; } else { self.v1 = 2; }
      packet.id = self.v1;
      packet.a1 = 0;
      for (let i = 0; i < 4; i += 1) {
        self.v2 += 1;
        if (self.v2 > 26) { self.v2 = 1; }
        packet.a2[i] = self.v2;
      }
      enqueue(s, packet)
    }
  }
}
class HandlerTask {
  v1: Packet?
  v2: Packet?
  implements Task {
    function run(self, s: Scheduler, packet: Packet?) -> Tcb? {
      if (packet != null) {
        if (packet.kind == 1) { self.v1 = add_to(packet, self.v1); }
        else { self.v2 = add_to(packet, self.v2); }
      }
      let w = self.v1;
      if (w != null) {
        let count = w.a1;
        if (count < 4) {
          let d = self.v2;
          if (d != null) {
            self.v2 = d.link;
            d.a1 = w.a2[count];
            w.a1 = count + 1;
            return enqueue(s, d);
          }
        } else {
          self.v1 = w.link;
          return enqueue(s, w);
        }
      }
      suspend_current(s)
    }
  }
}
function richards() -> int {
  let s = Scheduler { queue_count: 0, hold_count: 0, blocks: [null, null, null, null, null, null], list: null, current: null, current_id: 0 };
  let idle = add_task(s, 0, 0, null, IdleTask { v1: 1, count: 1000 });
  idle.state = 0;
  let q = packet(null, 1, 1);
  q = packet(q, 1, 1);
  add_task(s, 1, 1000, q, WorkerTask { v1: 2, v2: 0 });
  q = packet(null, 4, 0);
  q = packet(q, 4, 0);
  q = packet(q, 4, 0);
  add_task(s, 2, 2000, q, HandlerTask { v1: null, v2: null });
  q = packet(null, 5, 0);
  q = packet(q, 5, 0);
  q = packet(q, 5, 0);
  add_task(s, 3, 3000, q, HandlerTask { v1: null, v2: null });
  add_task(s, 4, 4000, null, DeviceTask { v1: null });
  add_task(s, 5, 5000, null, DeviceTask { v1: null });
  s.current = s.list;
  while (true) {
    let c = s.current;
    if (c == null) { break; }
    if (held_or_suspended(c)) {
      s.current = c.link;
    } else {
      s.current_id = c.id;
      s.current = tcb_run(c, s);
    }
  }
  s.queue_count * 10000 + s.hold_count
}
function main() -> int {
  let total = 0;
  for (let i = 0; i < 50; i += 1) { total += richards(); }
  total
}
```

## Python
```python
class Packet:
    __slots__ = ('link', 'id', 'kind', 'a1', 'a2')
    def __init__(self, link, id, kind):
        self.link = link; self.id = id; self.kind = kind; self.a1 = 0; self.a2 = [0, 0, 0, 0]
    def add_to(self, queue):
        self.link = None
        if queue is None: return self
        peek = queue
        while peek.link is not None: peek = peek.link
        peek.link = self
        return queue
class Tcb:
    __slots__ = ('link', 'id', 'priority', 'queue', 'task', 'state')
    def __init__(self, link, id, priority, queue, task):
        self.link = link; self.id = id; self.priority = priority; self.queue = queue; self.task = task
        self.state = 2 if queue is None else 3
    def run(self, s):
        p = None
        if self.state == 3:
            p = self.queue
            self.queue = p.link
            self.state = 0 if self.queue is None else 1
        return self.task.run(s, p)
    def check_priority_add(self, task, p):
        if self.queue is None:
            self.queue = p
            self.state |= 1
            if self.priority > task.priority: return self
        else:
            self.queue = p.add_to(self.queue)
        return task
    def held_or_suspended(self):
        return (self.state & 4) != 0 or self.state == 2
class Scheduler:
    def __init__(self):
        self.queue_count = 0; self.hold_count = 0; self.blocks = [None] * 6
        self.list = None; self.current = None; self.current_id = 0
    def add_task(self, id, priority, queue, task):
        t = Tcb(self.list, id, priority, queue, task)
        self.current = t; self.list = t; self.blocks[id] = t
        return t
    def release(self, id):
        t = self.blocks[id]
        if t is None: return None
        t.state &= -5
        if t.priority > self.current.priority: return t
        return self.current
    def hold_current(self):
        self.hold_count += 1
        self.current.state |= 4
        return self.current.link
    def suspend_current(self):
        self.current.state |= 2
        return self.current
    def enqueue(self, p):
        t = self.blocks[p.id]
        if t is None: return None
        self.queue_count += 1
        p.link = None
        p.id = self.current_id
        return t.check_priority_add(self.current, p)
class IdleTask:
    def __init__(self, v1, count): self.v1 = v1; self.count = count
    def run(self, s, packet):
        self.count -= 1
        if self.count == 0: return s.hold_current()
        if (self.v1 & 1) == 0:
            self.v1 = self.v1 >> 1
            return s.release(4)
        self.v1 = (self.v1 >> 1) ^ 53256
        return s.release(5)
class DeviceTask:
    def __init__(self): self.v1 = None
    def run(self, s, packet):
        if packet is None:
            v = self.v1
            if v is None: return s.suspend_current()
            self.v1 = None
            return s.enqueue(v)
        self.v1 = packet
        return s.hold_current()
class WorkerTask:
    def __init__(self, v1, v2): self.v1 = v1; self.v2 = v2
    def run(self, s, packet):
        if packet is None: return s.suspend_current()
        self.v1 = 3 if self.v1 == 2 else 2
        packet.id = self.v1
        packet.a1 = 0
        for i in range(4):
            self.v2 += 1
            if self.v2 > 26: self.v2 = 1
            packet.a2[i] = self.v2
        return s.enqueue(packet)
class HandlerTask:
    def __init__(self): self.v1 = None; self.v2 = None
    def run(self, s, packet):
        if packet is not None:
            if packet.kind == 1: self.v1 = packet.add_to(self.v1)
            else: self.v2 = packet.add_to(self.v2)
        w = self.v1
        if w is not None:
            count = w.a1
            if count < 4:
                d = self.v2
                if d is not None:
                    self.v2 = d.link
                    d.a1 = w.a2[count]
                    w.a1 = count + 1
                    return s.enqueue(d)
            else:
                self.v1 = w.link
                return s.enqueue(w)
        return s.suspend_current()
def richards():
    s = Scheduler()
    s.add_task(0, 0, None, IdleTask(1, 1000)).state = 0
    q = Packet(Packet(None, 1, 1), 1, 1)
    s.add_task(1, 1000, q, WorkerTask(2, 0))
    q = Packet(Packet(Packet(None, 4, 0), 4, 0), 4, 0)
    s.add_task(2, 2000, q, HandlerTask())
    q = Packet(Packet(Packet(None, 5, 0), 5, 0), 5, 0)
    s.add_task(3, 3000, q, HandlerTask())
    s.add_task(4, 4000, None, DeviceTask())
    s.add_task(5, 5000, None, DeviceTask())
    s.current = s.list
    while s.current is not None:
        c = s.current
        if c.held_or_suspended():
            s.current = c.link
        else:
            s.current_id = c.id
            s.current = c.run(s)
    return s.queue_count * 10000 + s.hold_count
print(sum(richards() for _ in range(50)))
```

## Typescript
```ts
class Packet {
  constructor(link, id, kind) { this.link = link; this.id = id; this.kind = kind; this.a1 = 0; this.a2 = [0, 0, 0, 0]; }
  addTo(queue) {
    this.link = null;
    if (queue === null) return this;
    let peek = queue;
    while (peek.link !== null) peek = peek.link;
    peek.link = this;
    return queue;
  }
}
class Tcb {
  constructor(link, id, priority, queue, task) {
    this.link = link; this.id = id; this.priority = priority; this.queue = queue; this.task = task;
    this.state = queue === null ? 2 : 3;
  }
  run(s) {
    let p = null;
    if (this.state === 3) {
      p = this.queue;
      this.queue = p.link;
      this.state = this.queue === null ? 0 : 1;
    }
    return this.task.run(s, p);
  }
  checkPriorityAdd(task, p) {
    if (this.queue === null) {
      this.queue = p;
      this.state |= 1;
      if (this.priority > task.priority) return this;
    } else {
      this.queue = p.addTo(this.queue);
    }
    return task;
  }
  heldOrSuspended() { return (this.state & 4) !== 0 || this.state === 2; }
}
class Scheduler {
  constructor() { this.queueCount = 0; this.holdCount = 0; this.blocks = [null, null, null, null, null, null]; this.list = null; this.current = null; this.currentId = 0; }
  addTask(id, priority, queue, task) {
    const t = new Tcb(this.list, id, priority, queue, task);
    this.current = t; this.list = t; this.blocks[id] = t;
    return t;
  }
  release(id) {
    const t = this.blocks[id];
    if (t === null) return null;
    t.state &= -5;
    return t.priority > this.current.priority ? t : this.current;
  }
  holdCurrent() { this.holdCount++; this.current.state |= 4; return this.current.link; }
  suspendCurrent() { this.current.state |= 2; return this.current; }
  enqueue(p) {
    const t = this.blocks[p.id];
    if (t === null) return null;
    this.queueCount++;
    p.link = null;
    p.id = this.currentId;
    return t.checkPriorityAdd(this.current, p);
  }
}
class IdleTask {
  constructor(v1, count) { this.v1 = v1; this.count = count; }
  run(s, packet) {
    this.count--;
    if (this.count === 0) return s.holdCurrent();
    if ((this.v1 & 1) === 0) { this.v1 = this.v1 >> 1; return s.release(4); }
    this.v1 = (this.v1 >> 1) ^ 53256;
    return s.release(5);
  }
}
class DeviceTask {
  constructor() { this.v1 = null; }
  run(s, packet) {
    if (packet === null) {
      const v = this.v1;
      if (v === null) return s.suspendCurrent();
      this.v1 = null;
      return s.enqueue(v);
    }
    this.v1 = packet;
    return s.holdCurrent();
  }
}
class WorkerTask {
  constructor(v1, v2) { this.v1 = v1; this.v2 = v2; }
  run(s, packet) {
    if (packet === null) return s.suspendCurrent();
    this.v1 = this.v1 === 2 ? 3 : 2;
    packet.id = this.v1;
    packet.a1 = 0;
    for (let i = 0; i < 4; i++) {
      this.v2++;
      if (this.v2 > 26) this.v2 = 1;
      packet.a2[i] = this.v2;
    }
    return s.enqueue(packet);
  }
}
class HandlerTask {
  constructor() { this.v1 = null; this.v2 = null; }
  run(s, packet) {
    if (packet !== null) {
      if (packet.kind === 1) this.v1 = packet.addTo(this.v1);
      else this.v2 = packet.addTo(this.v2);
    }
    const w = this.v1;
    if (w !== null) {
      const count = w.a1;
      if (count < 4) {
        const d = this.v2;
        if (d !== null) {
          this.v2 = d.link;
          d.a1 = w.a2[count];
          w.a1 = count + 1;
          return s.enqueue(d);
        }
      } else {
        this.v1 = w.link;
        return s.enqueue(w);
      }
    }
    return s.suspendCurrent();
  }
}
function richards() {
  const s = new Scheduler();
  s.addTask(0, 0, null, new IdleTask(1, 1000)).state = 0;
  s.addTask(1, 1000, new Packet(new Packet(null, 1, 1), 1, 1), new WorkerTask(2, 0));
  s.addTask(2, 2000, new Packet(new Packet(new Packet(null, 4, 0), 4, 0), 4, 0), new HandlerTask());
  s.addTask(3, 3000, new Packet(new Packet(new Packet(null, 5, 0), 5, 0), 5, 0), new HandlerTask());
  s.addTask(4, 4000, null, new DeviceTask());
  s.addTask(5, 5000, null, new DeviceTask());
  s.current = s.list;
  while (s.current !== null) {
    const c = s.current;
    if (c.heldOrSuspended()) s.current = c.link;
    else { s.currentId = c.id; s.current = c.run(s); }
  }
  return s.queueCount * 10000 + s.holdCount;
}
let total = 0;
for (let i = 0; i < 50; i++) total += richards();
console.log(total);
```
