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
