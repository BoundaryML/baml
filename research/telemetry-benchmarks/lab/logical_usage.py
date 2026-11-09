"""Apply the BCS projector billing rules (bcs 0c1735ab projector/billing.rs) to actual upload bodies.

Copied from ~/projects/trace-lab/baml/logical_usage.py (earlier session, 2026-11-06).
"""
import struct
from collections import Counter

from cloud_pb2 import CloudUploadEnvelope
from recording_pb2 import RecordingFile


FIELDS = {
    "function_announcement": "inputs_cas_id",
    "function_completion": "value_cas_id",
    "late_function_completion": "value_cas_id",
    "log": "data_cas_id",
    "network_announcement": "request_cas_id",
    "network_event": "payload_cas_id",
    "network_completion": "error_cas_id",
}


class Meter:
    def __init__(self):
        self.sizes = {}
        self.processes = set()
        self.names = Counter()
        self.roots = []
        self.files = set()
        self.uploads = set()
        # One entry per new upload envelope, in arrival order: what it carried and what it billed.
        self.timeline = []

    def ref(self, message, field):
        if message.HasField(field):
            root = getattr(message, field)
            self.roots.append(struct.pack("<QQ", root.low, root.high))

    def observe(self, body):
        envelope = CloudUploadEnvelope.FromString(body)
        if envelope.upload_id in self.uploads:
            return
        self.uploads.add(envelope.upload_id)
        entry = {"cas_bytes": sum(len(o.blob) for o in envelope.cas_objects), "cas_objects": len(envelope.cas_objects),
                 "body": len(body), "roots": [], "events": {}}
        self.timeline.append(entry)
        for obj in envelope.cas_objects:
            self.sizes[obj.snapshot_id] = obj.logical_bytes_approx_v1 if obj.HasField("logical_bytes_approx_v1") else len(obj.blob)
        if not envelope.HasField("recording_file"):
            return
        first_root = len(self.roots)
        file = RecordingFile.FromString(envelope.recording_file)
        identity = (file.header.recording_id, file.sequence)
        if identity in self.files:
            return
        self.files.add(identity)
        self.processes.add(file.header.process_id.hex())
        self.ref(file.header, "initial_context_cas_id")
        for section in file.spans.sections:
            self.ref(section, "context_cas_id")
            for event in section.events:
                kind = event.WhichOneof("event")
                self.names[kind] += 1
                entry["events"][kind] = entry["events"].get(kind, 0) + 1
                if kind in FIELDS:
                    self.ref(getattr(event, kind), FIELDS[kind])
        entry["roots"] = self.roots[first_root:]

    def summary(self):
        missing = sum(key not in self.sizes for key in self.roots)
        return {
            "spans": sum(self.names[key] for key in ("function_announcement", "network_announcement", "thread_announcement")),
            "logical_bytes": None if missing else sum(self.sizes[key] for key in self.roots),
            "capture_references": len(self.roots), "missing_sizes": missing,
            "process_ids": sorted(self.processes), "events": dict(self.names),
            "timeline": [{"cas_bytes": e["cas_bytes"], "cas_objects": e["cas_objects"], "body": e["body"],
                          "billed": sum(self.sizes.get(k, 0) for k in e["roots"]),
                          "network_announcements": e["events"].get("network_announcement", 0)}
                         for e in self.timeline],
        }
