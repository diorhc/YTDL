#!/usr/bin/env python3
"""
Configuration file for YTDL

This file contains all configuration constants and settings.
You can override these by setting environment variables.
"""

import os
import secrets
from pathlib import Path

# Application Settings
APP_NAME = "YTDL"
APP_VERSION = "2.1.0"
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

# Rate Limiting Settings
RATE_LIMIT_REQUESTS = int(os.environ.get('RATE_LIMIT_REQUESTS', '10'))  # Max requests
RATE_LIMIT_WINDOW = int(os.environ.get('RATE_LIMIT_WINDOW', '60'))  # Per seconds

# Validation Settings
MAX_URL_RETRIES = int(os.environ.get('MAX_URL_RETRIES', '3'))
DOWNLOAD_TIMEOUT = int(os.environ.get('DOWNLOAD_TIMEOUT', '1800'))  # 30 minutes


def validate_config() -> bool:
    """Validate configuration settings."""
    errors = []
    
    # Validate numeric ranges
    if MAX_RETRIES < 1 or MAX_RETRIES > 10:
        errors.append(f"MAX_RETRIES must be between 1 and 10, got {MAX_RETRIES}")
    
    if VIDEO_INFO_CACHE_TTL < 0 or VIDEO_INFO_CACHE_TTL > 3600:
        errors.append(f"VIDEO_INFO_CACHE_TTL must be between 0 and 3600, got {VIDEO_INFO_CACHE_TTL}")
    
    if FLASK_PORT < 1 or FLASK_PORT > 65535:
        errors.append(f"FLASK_PORT must be between 1 and 65535, got {FLASK_PORT}")
    
    if HTTP_CHUNK_SIZE < 1024 or HTTP_CHUNK_SIZE > 104857600:  # 1KB to 100MB
        errors.append(f"HTTP_CHUNK_SIZE must be between 1KB and 100MB, got {HTTP_CHUNK_SIZE}")
    
    # Validate paths
    try:
        DOWNLOAD_PATH.mkdir(exist_ok=True, parents=True)
    except Exception as e:
        errors.append(f"Cannot create DOWNLOAD_PATH: {e}")
    
    if errors:
        print("Configuration validation errors:")
        for error in errors:
            print(f"  - {error}")
        return False
    
    return True


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
