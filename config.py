#!/usr/bin/env python3
"""
Configuration file for YTDL

This file contains all configuration constants and settings.
You can override these by setting environment variables.
"""

import os
from pathlib import Path

# Application Settings
APP_NAME = "YTDL"
APP_VERSION = "2.0.0"
DEBUG_MODE = os.environ.get('DEBUG_MODE', 'false').lower() in ('true', '1', 'yes', 'on')

# Flask Settings
FLASK_SECRET_KEY = os.environ.get('FLASK_SECRET_KEY')
FLASK_HOST = os.environ.get('FLASK_HOST', '0.0.0.0')
FLASK_PORT = int(os.environ.get('FLASK_PORT', '5005'))

# Download Settings
DOWNLOAD_PATH = Path(os.environ.get('DOWNLOAD_PATH', './downloads'))
MAX_FILE_SIZE = 10 * 1024 * 1024 * 1024  # 10GB default limit
DEFAULT_QUALITY = os.environ.get('DEFAULT_QUALITY', 'best')

# Cache Settings
VIDEO_INFO_CACHE_TTL = int(os.environ.get('CACHE_TTL', '300'))  # 5 minutes
MAX_CACHE_SIZE = int(os.environ.get('MAX_CACHE_SIZE', '100'))

# Security Settings
ALLOWED_SCHEMES = ['http', 'https']
MAX_URL_LENGTH = 4096
MAX_FILENAME_LENGTH = 255
INSECURE_SSL = os.environ.get('INSECURE_SSL', 'false').lower() in ('true', '1', 'yes', 'on')

# Logging Settings
LOG_LEVEL = os.environ.get('LOG_LEVEL', 'INFO')
LOG_FORMAT = '%(asctime)s - %(name)s - %(levelname)s - %(message)s'
LOG_FILE = os.environ.get('LOG_FILE', 'youtube_downloader.log')

# FFmpeg Settings
FFMPEG_TIMEOUT = int(os.environ.get('FFMPEG_TIMEOUT', '600'))  # 10 minutes
FFPROBE_TIMEOUT = int(os.environ.get('FFPROBE_TIMEOUT', '30'))

# Download Retry Settings
MAX_RETRIES = int(os.environ.get('MAX_RETRIES', '3'))
RETRY_DELAY_MIN = int(os.environ.get('RETRY_DELAY_MIN', '1'))
RETRY_DELAY_MAX = int(os.environ.get('RETRY_DELAY_MAX', '5'))

# Request Settings
REQUEST_TIMEOUT = int(os.environ.get('REQUEST_TIMEOUT', '45'))
SOCKET_TIMEOUT = int(os.environ.get('SOCKET_TIMEOUT', '60'))

# Performance Settings
HTTP_CHUNK_SIZE = int(os.environ.get('HTTP_CHUNK_SIZE', '10485760'))  # 10MB


def get_config_summary() -> dict:
    """Get a summary of current configuration."""
    return {
        'app_name': APP_NAME,
        'version': APP_VERSION,
        'debug_mode': DEBUG_MODE,
        'download_path': str(DOWNLOAD_PATH),
        'default_quality': DEFAULT_QUALITY,
        'cache_ttl': VIDEO_INFO_CACHE_TTL,
        'max_retries': MAX_RETRIES,
        'insecure_ssl': INSECURE_SSL,
    }


def print_config():
    """Print current configuration to console."""
    print(f"\n{'='*60}")
    print(f"  {APP_NAME} v{APP_VERSION} - Configuration")
    print(f"{'='*60}")
    for key, value in get_config_summary().items():
        print(f"  {key:20s}: {value}")
    print(f"{'='*60}\n")
