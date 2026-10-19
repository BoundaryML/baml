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
function main() {
  let total = 0;
  for (let i = 0; i < 50; i++) total += richards();
  return total;
}
