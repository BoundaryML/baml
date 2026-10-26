from enum import Enum


class Status(Enum):
    TODO = 0
    DOING = 1
    REVIEW = 2
    DONE = 3


class Task:
    __slots__ = ("id", "status")

    def __init__(self, id, status):
        self.id = id
        self.status = status


def advance(s, n):
    if s == Status.TODO:
        return Status.DOING
    if s == Status.DOING:
        if n % 3 == 0:
            return Status.REVIEW
        return Status.DOING
    if s == Status.REVIEW:
        if n % 5 == 0:
            return Status.DOING
        return Status.DONE
    return Status.TODO


def main():
    tasks = [Task(i, Status.TODO) for i in range(1000)]
    done = 0
    for round in range(40):
        for t in tasks:
            t.status = advance(t.status, t.id + round)
            if t.status == Status.DONE:
                done += t.id % 7 + 1
    return done
