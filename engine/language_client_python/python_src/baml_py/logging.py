from .baml_py import set_log_level, set_log_json_mode, get_log_level, set_log_max_chunk_length

# Alias to match docs naming
set_log_max_message_length = set_log_max_chunk_length

__all__ = [
    "set_log_level",
    "set_log_json_mode",
    "get_log_level",
    "set_log_max_chunk_length",
    "set_log_max_message_length",
]
