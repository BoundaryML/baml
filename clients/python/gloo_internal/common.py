from __future__ import annotations
from datetime import datetime
import uuid


# Define the named tuple 'Event'
class EventBase:
    func_name: str
    variant_name: str | None
    timestamp: datetime
    event_id: str
    parent_event_id: str | None

    def __init__(
        self, *, func_name: str, variant_name: str | None, parent_event_id: str | None
    ):
        self.func_name = func_name
        self.variant_name = variant_name
        self.event_id = str(uuid.uuid4())
        self.timestamp = datetime.utcnow()
        self.parent_event_id = parent_event_id
