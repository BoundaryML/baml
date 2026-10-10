"""Providers: run `python3 -m bench execute` on a machine and bring results back."""
import json
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path


def log(*a):
    print(time.strftime("%H:%M:%S"), *a, file=sys.stderr, flush=True)


# -I would ignore PYTHONPATH and the cwd; the package is shipped beside the stage, so cwd is the path.
EXECUTE = "cd {home}/ship && mkdir -p results && setsid -f bash -c 'export PATH=$HOME/.cargo/bin:$PATH; {python} -m bench execute --stage . --machine machine.json --protocol protocol.json --out results > results/execute.log 2>&1; echo $? > results/execute.exit; touch results/execute.done' < /dev/null > /dev/null 2>&1; echo started"


class Local:
    def __init__(self, machine):
        self.machine = machine

    def run(self, stage, out):
        root = Path(self.machine.data["bench_root"]).expanduser()
        root.mkdir(parents=True, exist_ok=True)
        cmd = [sys.executable, "-m", "bench", "execute", "--stage", str(stage), "--machine", str(stage / "machine.json"),
               "--protocol", str(stage / "protocol.json"), "--out", str(out / "results"), "--root", str(root)]
        (out / "results").mkdir(parents=True, exist_ok=True)
        with (out / "results" / "execute.log").open("w") as logf:
            r = subprocess.run(cmd, cwd=str(stage), stdout=logf, stderr=subprocess.STDOUT)
        return r.returncode


class Freestyle:
    def __init__(self, machine):
        self.machine = machine
        self.env = {**os.environ, "FREESTYLE_API_KEY": os.environ.get("FREESTYLE_API_KEY") or (Path.home() / ".config/freestyle/api.key").read_text().strip(),
                    "FREESTYLE_TEAM": machine.data["team"]}
        self.vm = None

    def fs(self, *args, capture=True):
        return subprocess.run(["npx", "-y", "freestyle@latest", *args], env=self.env, capture_output=capture, text=True, stdin=subprocess.DEVNULL)

    def exec(self, *cmd, user=None):
        extra = ["--linux-user", user] if user else []
        return self.fs("vm", "exec", self.vm, *extra, "--", *cmd).stdout

    def run(self, stage, out):
        home = self.machine.data["guest_home"]
        log("creating ephemeral VM from", self.machine.data["snapshot_id"])
        r = self.fs("vm", "create", "--snapshot-id", self.machine.data["snapshot_id"], "--ephemeral", "--no-ssh", "--output", "json")
        d = json.loads(r.stdout)
        self.vm = d.get("vmId") or d.get("id") or (d.get("vm") or {}).get("id")
        if not self.vm:
            raise RuntimeError(f"no vm id in {r.stdout[:300]} {r.stderr[:300]}")
        log("vm", self.vm)
        try:
            for _ in range(30):
                if "guest-ready" in self.exec("echo", "guest-ready"):
                    break
                time.sleep(5)
            (out / "machine.txt").write_text(self.exec("bash", "-c", "grep -m1 'model name' /proc/cpuinfo; uname -r; cat /proc/loadavg"))
            # Files the snapshot was seeded with may belong to root; the run writes beside them as the default user.
            self.exec("chown", "-R", "1000:1000", self.machine.data["bench_root"], user="root")
            log("shipping", stage)
            self.exec("bash", "-c", f"rm -rf {home}/ship && mkdir -p {home}/ship")
            for attempt in range(3):
                r = self.fs("vm", "scp", f"{stage}/.", f"{self.vm}:{home}/ship")
                listing = self.exec("ls", f"{home}/ship")
                if r.returncode == 0 and "protocol.json" in listing and "src.tgz" in listing:
                    break
                log(f"scp attempt {attempt + 1} failed: rc={r.returncode} {r.stderr.strip()[-200:]}")
                time.sleep(5)
            else:
                raise RuntimeError("shipping the stage failed three times")
            # scp writes as root; the run executes as the default user.
            self.exec("chown", "-R", "1000:1000", f"{home}/ship", user="root")
            self.exec("bash", "-c", EXECUTE.format(home=home, python=self.machine.data.get("python", "python3")))
            last = ""
            while "execute.done" not in self.exec("ls", f"{home}/ship/results"):
                time.sleep(30)
                tail = self.exec("bash", "-c", f"tail -n 1 {home}/ship/results/execute.log 2>/dev/null").strip()
                if tail != last:
                    log("guest:", tail[-160:])
                    last = tail
            self.exec("bash", "-c", f"cd {home}/ship && tar czf /tmp/results.tgz results")
            self.fs("vm", "scp", f"{self.vm}:/tmp/results.tgz", str(out / "results.tgz"))
            subprocess.run(["tar", "xzf", str(out / "results.tgz"), "-C", str(out)], check=True)
            (out / "results.tgz").unlink()
            return int((out / "results" / "execute.exit").read_text().strip() or 1)
        finally:
            log("deleting", self.vm)
            self.fs("vm", "delete", self.vm, "--output", "json")


class Ssh:
    def __init__(self, machine):
        self.machine = machine
        self.host = machine.data["host_ssh"]

    def ssh(self, cmd):
        return subprocess.run(["ssh", self.host, cmd], capture_output=True, text=True).stdout

    def run(self, stage, out):
        home = self.machine.data.get("guest_home", "~")
        self.ssh(f"rm -rf {home}/ship && mkdir -p {home}/ship")
        subprocess.run(["rsync", "-a", f"{stage}/.", f"{self.host}:{home}/ship/"], check=True)
        (out / "machine.txt").write_text(self.ssh("grep -m1 'model name' /proc/cpuinfo; uname -r; cat /proc/loadavg"))
        self.ssh(EXECUTE.format(home=home, python=self.machine.data.get("python", "python3")))
        while "execute.done" not in self.ssh(f"ls {home}/ship/results"):
            time.sleep(30)
        subprocess.run(["rsync", "-a", f"{self.host}:{home}/ship/results", str(out) + "/"], check=True)
        return int((out / "results" / "execute.exit").read_text().strip() or 1)


def provider(machine):
    return {"local": Local, "freestyle": Freestyle, "ssh": Ssh}[machine.provider](machine)
