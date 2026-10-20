class Packet:
    __slots__ = ('link', 'id', 'kind', 'a1', 'a2')

    def __init__(self, link, id, kind):
        self.link = link
        self.id = id
        self.kind = kind
        self.a1 = 0
        self.a2 = [0, 0, 0, 0]

    def add_to(self, queue):
        self.link = None
        if queue is None:
            return self
        peek = queue
        while peek.link is not None:
            peek = peek.link
        peek.link = self
        return queue

class Tcb:
    __slots__ = ('link', 'id', 'priority', 'queue', 'task', 'state')

    def __init__(self, link, id, priority, queue, task):
        self.link = link
        self.id = id
        self.priority = priority
        self.queue = queue
        self.task = task
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
            if self.priority > task.priority:
                return self
        else:
            self.queue = p.add_to(self.queue)
        return task

    def held_or_suspended(self):
        return self.state & 4 != 0 or self.state == 2

class Scheduler:

    def __init__(self):
        self.queue_count = 0
        self.hold_count = 0
        self.blocks = [None] * 6
        self.list = None
        self.current = None
        self.current_id = 0

    def add_task(self, id, priority, queue, task):
        t = Tcb(self.list, id, priority, queue, task)
        self.current = t
        self.list = t
        self.blocks[id] = t
        return t

    def release(self, id):
        t = self.blocks[id]
        if t is None:
            return None
        t.state &= -5
        if t.priority > self.current.priority:
            return t
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
        if t is None:
            return None
        self.queue_count += 1
        p.link = None
        p.id = self.current_id
        return t.check_priority_add(self.current, p)

class IdleTask:

    def __init__(self, v1, count):
        self.v1 = v1
        self.count = count

    def run(self, s, packet):
        self.count -= 1
        if self.count == 0:
            return s.hold_current()
        if self.v1 & 1 == 0:
            self.v1 = self.v1 >> 1
            return s.release(4)
        self.v1 = self.v1 >> 1 ^ 53256
        return s.release(5)

class DeviceTask:

    def __init__(self):
        self.v1 = None

    def run(self, s, packet):
        if packet is None:
            v = self.v1
            if v is None:
                return s.suspend_current()
            self.v1 = None
            return s.enqueue(v)
        self.v1 = packet
        return s.hold_current()

class WorkerTask:

    def __init__(self, v1, v2):
        self.v1 = v1
        self.v2 = v2

    def run(self, s, packet):
        if packet is None:
            return s.suspend_current()
        self.v1 = 3 if self.v1 == 2 else 2
        packet.id = self.v1
        packet.a1 = 0
        for i in range(4):
            self.v2 += 1
            if self.v2 > 26:
                self.v2 = 1
            packet.a2[i] = self.v2
        return s.enqueue(packet)

class HandlerTask:

    def __init__(self):
        self.v1 = None
        self.v2 = None

    def run(self, s, packet):
        if packet is not None:
            if packet.kind == 1:
                self.v1 = packet.add_to(self.v1)
            else:
                self.v2 = packet.add_to(self.v2)
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

def main():
    return sum((richards() for _ in range(50)))
