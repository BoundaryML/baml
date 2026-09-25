#!/bin/sh
set -eu
if [ -f /sys/fs/cgroup/cgroup.controllers ]; then
    mkdir -p /sys/fs/cgroup/miniatb-controller
    printf '%s\n' "$$" > /sys/fs/cgroup/miniatb-controller/cgroup.procs
fi
node --test deployment/*.test.mjs
baml run --agent-skill-check off test_sandbox
baml run --agent-skill-check off test_replay
