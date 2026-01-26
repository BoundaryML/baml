"""Minimal Django settings for BAML observability test."""
import os

SECRET_KEY = "test-secret-key-not-for-production"

DEBUG = True

INSTALLED_APPS = [
    "django_rq",
]

# Redis host - use 'redis' in Docker, 'localhost' otherwise
REDIS_HOST = os.environ.get("REDIS_HOST", "localhost")

# Redis Queue configuration
RQ_QUEUES = {
    "default": {
        "HOST": REDIS_HOST,
        "PORT": 6379,
        "DB": 0,
    },
    "async": {
        "HOST": REDIS_HOST,
        "PORT": 6379,
        "DB": 0,
        "ASYNC": True,  # Enable async job support
    },
}

# Required for Django
USE_TZ = True
