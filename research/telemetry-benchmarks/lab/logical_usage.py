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

    def ref(self, message, field):
        if message.HasField(field):
            root = getattr(message, field)
            self.roots.append(struct.pack("<QQ", root.low, root.high))

    def observe(self, body):
        envelope = CloudUploadEnvelope.FromString(body)
        if envelope.upload_id in self.uploads:
            return
        self.uploads.add(envelope.upload_id)
        for obj in envelope.cas_objects:
            self.sizes[obj.snapshot_id] = obj.logical_bytes_approx_v1 if obj.HasField("logical_bytes_approx_v1") else len(obj.blob)
        if not envelope.HasField("recording_file"):
            return
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
                if kind in FIELDS:
                    self.ref(getattr(event, kind), FIELDS[kind])

    def summary(self):
        missing = sum(key not in self.sizes for key in self.roots)
        return {
            "spans": sum(self.names[key] for key in ("function_announcement", "network_announcement", "thread_announcement")),
            "logical_bytes": None if missing else sum(self.sizes[key] for key in self.roots),
            "capture_references": len(self.roots), "missing_sizes": missing,
            "process_ids": sorted(self.processes), "events": dict(self.names),
        }
