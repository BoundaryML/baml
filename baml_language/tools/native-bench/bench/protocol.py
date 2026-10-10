"""Protocol and machine files: data, loaded and hashed into every result."""
import hashlib
import json
from dataclasses import dataclass, field
from pathlib import Path


def sha256_file(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


@dataclass(frozen=True)
class Scenario:
    name: str
    case: str
    jobs: int
    warmup: int
    size: int = 64
    variant: str = "iterator"
    batches: int = 8
    retain: int = 2
    fanout: int = 8
    delay_ms: int = 10

    def fixture_args(self, seed):
        return (self.case, self.size, self.variant, self.batches, self.retain, self.fanout, self.delay_ms, seed)


@dataclass
class Protocol:
    path: Path
    sha256: str
    name: str
    rows: list
    trials: int
    seed: int
    cpu: int
    workers: int
    timeout_s: int
    cases: list
    scenarios: list
    regression: dict = field(default_factory=dict)

    @classmethod
    def load(cls, path):
        path = Path(path)
        d = json.loads(path.read_text())
        if d.get("schema") != 1:
            raise ValueError(f"{path}: unknown protocol schema {d.get('schema')}")
        scenarios = [Scenario(**s) for s in d["scenarios"]]
        names = [s.name for s in scenarios]
        if len(set(names)) != len(names):
            raise ValueError(f"{path}: duplicate scenario names")
        for s in scenarios:
            if s.case not in d["cases"]:
                raise ValueError(f"{path}: scenario {s.name} uses case {s.case} not in cases")
        return cls(path=path, sha256=sha256_file(path), name=d["name"], rows=d["rows"], trials=d["trials"],
                   seed=d["seed"], cpu=d["cpu"], workers=d["workers"], timeout_s=d.get("timeout_s", 600),
                   cases=d["cases"], scenarios=scenarios, regression=d.get("regression", {}))


@dataclass
class Machine:
    path: Path
    sha256: str
    name: str
    provider: str
    data: dict

    @classmethod
    def load(cls, path):
        path = Path(path)
        d = json.loads(path.read_text())
        if d.get("schema") != 1:
            raise ValueError(f"{path}: unknown machine schema")
        return cls(path=path, sha256=sha256_file(path), name=d["name"], provider=d["provider"], data=d)

    @property
    def bench_root(self):
        return Path(self.data["bench_root"]).expanduser()

    def tool(self, name):
        p = Path(self.data["tools"][name]).expanduser()
        return p if p.is_absolute() else self.bench_root / p
