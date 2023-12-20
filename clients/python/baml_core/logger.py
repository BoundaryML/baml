# Set up module-specific logging
import logging

logger = logging.getLogger("baml_core")
baml_client_logger = logging.getLogger("baml_client")
logger.setLevel(logging.WARNING)
baml_client_logger.setLevel(logging.ERROR)

# Custom field styles for coloredlogs
field_styles = {
    "asctime": {"color": "green"},
    "hostname": {"color": "magenta"},
    "levelname": {"color": "white", "bold": True},
    "name": {"color": "blue", "bold": True},
    "programname": {"color": "cyan"},
}
level_styles = {
    "warning": {"color": "yellow"},
    "error": {"color": "red"},
    "critical": {"color": "red", "bold": True},
}
