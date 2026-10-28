"""Stage exact release executables without extracting archive-controlled paths."""

from dataclasses import dataclass
from enum import Enum
from pathlib import Path
import tarfile
import zipfile


class WrapperVariant(str, Enum):
    SELF_UPDATE = "self-update"
    NO_SELF_UPDATE = "no-self-update"

    @property
    def enabled(self):
        return self is WrapperVariant.SELF_UPDATE

    @property
    def suffix(self):
        return "" if self.enabled else "-no-self-update"


@dataclass(frozen=True)
class ReleaseExecutable:
    archive: Path
    binary_name: str

    def read(self):
        if self.archive.suffix == ".zip":
            with zipfile.ZipFile(self.archive) as source:
                members = [
                    entry
                    for entry in source.infolist()
                    if entry.filename.removeprefix("./") == self.binary_name
                    and not entry.is_dir()
                ]
                if len(members) != 1:
                    raise ValueError(f"{self.archive}: expected one {self.binary_name}")
                return source.read(members[0])
        with tarfile.open(self.archive) as source:
            members = [
                entry
                for entry in source.getmembers()
                if entry.name.removeprefix("./") == self.binary_name and entry.isfile()
            ]
            if len(members) != 1:
                raise ValueError(
                    f"{self.archive}: expected one regular {self.binary_name}"
                )
            with source.extractfile(members[0]) as binary:
                return binary.read()


@dataclass(frozen=True)
class CandidateStager:
    artifacts: Path
    destination: Path
    platform: dict
    version: str

    def stage(self, implementation, product, variant):
        target = self.platform["triple"]
        executable_suffix = ".exe" if self.platform["os"] == "windows" else ""
        label = f"{implementation}{variant.suffix}"
        archive_name = f"{product}{variant.suffix}-{self.version}-{target}{self.platform['archive_suffix']}"
        matches = list(self.artifacts.rglob(archive_name))
        if len(matches) != 1:
            raise ValueError(
                f"expected exactly one {archive_name}, found {len(matches)}"
            )
        executable = ReleaseExecutable(matches[0], f"bin/baml{executable_suffix}")
        staged = self.destination / f"{label}{executable_suffix}"
        staged.write_bytes(executable.read())
        staged.chmod(0o755)
        return {
            "label": label,
            "command": [str(staged.resolve())],
            "self_update": variant.enabled,
            "version": self.version,
            "posthog": True,
        }


def candidates_from_archives(artifacts, destination, platform, version):
    stager = CandidateStager(Path(artifacts), Path(destination), platform, version)
    variants = [
        WrapperVariant(value) for value in platform["artifacts"]["wrapper"]["variants"]
    ]
    return [
        stager.stage(implementation, product, variant)
        for implementation, product in [
            ("rust", "baml-wrapper"),
            ("baml", "baml-wrapper-baml"),
        ]
        for variant in variants
    ]
