#!/usr/bin/env python3
"""
YTDL Web Interface

Flask-based web interface for multi-platform video downloads.
Features:
- Real-time progress tracking via Server-Sent Events (SSE)
- Multiple quality options
- Audio-only downloads
- Download history
- Secure file handling
"""

import os
import json
import threading
import sys
import uuid
import subprocess
import logging
from datetime import datetime
from typing import Dict, Any, Optional, Tuple
from pathlib import Path

# Add current directory to Python path for imports
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

try:
    import config
    # Validate configuration on startup
    if not config.validate_config():
        print("Warning: Configuration validation failed. Using defaults.")
except ImportError:
    # Fallback if config.py is not available
    class config:
        DEBUG_MODE = False
        VIDEO_INFO_CACHE_TTL = 300
        MAX_RETRIES = 3

# Configure logging based on DEBUG_MODE
logging.basicConfig(
    level=logging.DEBUG if config.DEBUG_MODE else logging.WARNING,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

# Disable werkzeug logs when DEBUG_MODE is off
if not config.DEBUG_MODE:
    log = logging.getLogger('werkzeug')
    log.setLevel(logging.ERROR)  # Only show errors, not INFO

from flask import Flask, render_template, request, jsonify, send_file, make_response, Response
import time
from collections import defaultdict
from functools import wraps

from youtube_downloader import YouTubeDownloader

# Initialize Flask app with optimized configuration
app = Flask(__name__)

# Rate limiting implementation
_rate_limit_data: Dict[str, list] = defaultdict(list)
_rate_limit_lock = threading.Lock()


def get_client_ip() -> str:
    """Get client IP address, considering proxy headers."""
    # Check for X-Forwarded-For header (common in proxy setups)
    if request.headers.get('X-Forwarded-For'):
        return request.headers.get('X-Forwarded-For').split(',')[0].strip()
    # Check for X-Real-IP header
    if request.headers.get('X-Real-IP'):
        return request.headers.get('X-Real-IP')
    return request.remote_addr or '127.0.0.1'


def rate_limit(max_requests: int = 10, window_seconds: int = 60):
    """Rate limiting decorator for API endpoints."""
    def decorator(f):
        @wraps(f)
        def wrapped(*args, **kwargs):
            client_ip = get_client_ip()
            current_time = time.time()
            
            with _rate_limit_lock:
                # Clean old entries
                _rate_limit_data[client_ip] = [
                    t for t in _rate_limit_data[client_ip]
                    if current_time - t < window_seconds
                ]
                
                # Check rate limit
                if len(_rate_limit_data[client_ip]) >= max_requests:
                    logger.warning(f"Rate limit exceeded for {client_ip}")
                    return jsonify({
                        'error': 'Rate limit exceeded. Please wait before making more requests.',
                        'retry_after': window_seconds
                    }), 429
                
                # Record this request
                _rate_limit_data[client_ip].append(current_time)
            
            return f(*args, **kwargs)
        return wrapped
    return decorator


# SSE: Stream download progress updates
@app.route('/api/progress_sse/<download_id>')
def progress_sse(download_id: str):
    def event_stream():
        last_progress = None
        while True:
            if download_id in active_downloads:
                progress = active_downloads[download_id]
            elif download_id in completed_downloads:
                progress = completed_downloads[download_id]
            else:
                yield "event: error\ndata: Download not found\n\n"
                break
            # Only send if progress changed
            if progress != last_progress:
                # Always include download_id and filename in the final event if possible
                if progress.get('status') == 'completed':
                    progress = progress.copy()
                    progress['download_id'] = download_id
                    # Persist file_path if present and not already stored
                    fp = progress.get('file_path')
                    if fp:
                        try:
                            # Save resolved absolute path into completed_downloads
                            resolved = str(Path(fp).resolve())
                            if download_id in completed_downloads:
                                completed_downloads[download_id]['file_path'] = resolved
                            elif download_id in active_downloads:
                                active_downloads[download_id]['file_path'] = resolved
                            progress['file_path'] = resolved
                        except Exception:
                            pass
                    if 'filename' not in progress or not progress['filename']:
                        # Try to get filename from completed_downloads
                        cd = completed_downloads.get(download_id)
                        if cd and 'filename' in cd:
                            progress['filename'] = cd['filename']
                yield f"data: {json.dumps(progress)}\n\n"
                last_progress = progress.copy()
            if progress.get('status') in ['completed', 'error']:
                break
            time.sleep(1)
    return Response(event_stream(), mimetype='text/event-stream')

# Generate or load secret key securely
def get_secret_key():
    """Get secret key from environment or generate a persistent one."""
    import secrets as secrets_module
    
    secret_key = os.environ.get('FLASK_SECRET_KEY')
    if secret_key:
        return secret_key.encode() if isinstance(secret_key, str) else secret_key
    
    # Generate a persistent key file for development
    key_file = Path('.secret_key')
    if key_file.exists():
        try:
            key_data = key_file.read_bytes()
            # Validate key length
            if len(key_data) >= 32:
                return key_data
            # Key too short, regenerate
            logger.warning("Secret key file too short, regenerating")
        except Exception as e:
            logger.warning(f"Could not read secret key file: {e}")
    
    # Generate new cryptographically secure key and save it
    try:
        new_key = secrets_module.token_bytes(32)
        key_file.write_bytes(new_key)
        # Restrict permissions (works on Unix, ignored on Windows)
        try:
            key_file.chmod(0o600)
        except OSError:
            pass  # Windows doesn't support chmod the same way
        return new_key
    except Exception as e:
        logger.warning(f"Could not write secret key file: {e}")
        # Fall back to in-memory key
        return secrets_module.token_bytes(32)

app.config.update(
    SECRET_KEY=get_secret_key(),
    MAX_CONTENT_LENGTH=16 * 1024 * 1024,  # 16MB max
    JSON_SORT_KEYS=False,
    JSONIFY_PRETTYPRINT_REGULAR=False,  # Optimize JSON responses
)

# Global variables for tracking downloads (optimized structure)
active_downloads: Dict[str, Dict[str, Any]] = {}
completed_downloads: Dict[str, Dict[str, Any]] = {}

# Simple cache for video info to reduce repeated API calls
_video_info_cache: Dict[str, Tuple[Dict[str, Any], float]] = {}
_cache_ttl = 300  # 5 minutes cache TTL

def _get_cached_video_info(url: str) -> Optional[Dict[str, Any]]:
    """
    Get cached video info if available and not expired.
    
    Args:
        url: Video URL to look up in cache
        
    Returns:
        Cached video info dict if found and valid, None otherwise
    """
    if url in _video_info_cache:
        info, timestamp = _video_info_cache[url]
        if time.time() - timestamp < _cache_ttl:
            return info
        else:
            # Remove expired cache entry
            del _video_info_cache[url]
    return None

def _cache_video_info(url: str, info: Dict[str, Any]) -> None:
    """
    Cache video info with timestamp.
    
    Args:
        url: Video URL as cache key
        info: Video information dict to cache
        
    Note:
        Automatically manages cache size (max 100 entries)
        and removes oldest entries when limit is reached.
    """
    _video_info_cache[url] = (info, time.time())
    # Limit cache size to prevent memory issues
    if len(_video_info_cache) > 100:
        # Remove oldest entries
        sorted_items = sorted(_video_info_cache.items(), key=lambda x: x[1][1])
        for old_url, _ in sorted_items[:20]:
            del _video_info_cache[old_url]

class WebDownloader(YouTubeDownloader):
    """Web version of the multi-platform downloader."""
    
    def __init__(self, download_path: str = "./downloads"):
        super().__init__(download_path)
        self.download_id: Optional[str] = None
    
    def set_download_id(self, download_id: str) -> None:
        """Set download ID for progress tracking."""
        self.download_id = download_id
        # Store audio_only flag for frontend display
        active_downloads[download_id] = {
            'status': 'starting',
            'progress': 0,
            'filename': '',
            'ultra_mode': self.merger.available,
            'error': None,
            'started_at': datetime.now().isoformat(),
            'downloaded_bytes': 0,
            'total_bytes': 0,
            'audio_only': getattr(self, 'audio_only', False)
        }
        # Set up progress callback
        self.set_progress_hook(self._web_progress_hook)
    
    def _web_progress_hook(self, d: Dict[str, Any]) -> None:
        """Optimized progress hook for web interface."""
        if not self.download_id or self.download_id not in active_downloads:
            return
        download_info = active_downloads[self.download_id]
        try:
            if d['status'] == 'downloading':
                download_info['status'] = 'downloading'
                download_info['downloaded_bytes'] = d.get('downloaded_bytes', 0)
                # Propagate audio_only flag for frontend
                if hasattr(self, 'audio_only'):
                    download_info['audio_only'] = self.audio_only
                if 'total_bytes' in d and d['total_bytes']:
                    download_info['total_bytes'] = d['total_bytes']
                    download_info['progress'] = (d['downloaded_bytes'] / d['total_bytes']) * 100
            elif d['status'] == 'finished':
                # Check if this is just a segment finishing or the entire download
                if 'fragment_index' in d or 'fragment_count' in d:
                    # This is just a fragment/segment finishing, not the entire download
                    return
                
                # For video downloads, don't mark as complete if this is just audio finishing
                # Check if filename suggests this is just one part of a multi-part download
                filename = d.get('filename', '')
                # Check if this is a temp file (indicating separate stream download)
                is_temp_file = 'temp' in filename.lower() or 'tmp' in filename.lower()
                is_partial_stream = ('audio' in filename.lower() or 'video' in filename.lower() or 
                                   filename.endswith('.webm') or filename.endswith('.m4a') or is_temp_file)
                
                if (not getattr(self, 'audio_only', False) and is_partial_stream):
                    # This is likely just audio/video portion finishing, not complete download
                    download_info['status'] = 'processing'
                    return
                
                # Only mark as completed when the entire download is actually finished
                filename = Path(d['filename']).name
                download_info.update({
                    'status': 'completed',
                    'progress': 100,  # Ensure 100% progress
                    'filename': filename,
                    'file_path': d['filename']
                })
                # Propagate audio_only flag for frontend
                if hasattr(self, 'audio_only'):
                    download_info['audio_only'] = self.audio_only
                # Move to completed downloads only when truly finished
                completed_downloads[self.download_id] = download_info.copy()
                del active_downloads[self.download_id]
        except Exception as e:
            logger.error(f'Exception in _web_progress_hook: {e}', exc_info=True)
            # Mark download as error if exception occurs
            if self.download_id in active_downloads:
                active_downloads[self.download_id]['status'] = 'error'
                active_downloads[self.download_id]['error'] = str(e)


# Global downloader instance
downloader = WebDownloader()

# Security helper functions
def validate_safe_path(requested_path: str, base_dir: Path) -> Optional[Path]:
    """
    Validate that requested_path is safely within base_dir.
    Returns resolved Path if safe, None otherwise.
    Prevents path traversal attacks.
    
    Args:
        requested_path: Requested file path (relative or filename)
        base_dir: Base directory that must contain the file
        
    Returns:
        Resolved Path if safe, None otherwise
    """
    if not requested_path or not isinstance(requested_path, str):
        logger.warning("Invalid path: empty or not a string")
        return None
    
    # Check for suspicious characters
    suspicious_chars = ['\x00', '\r', '\n', '\t']
    if any(char in requested_path for char in suspicious_chars):
        logger.warning(f"Suspicious characters in path: {repr(requested_path)}")
        return None
    
    # Check for directory traversal attempts
    if '..' in requested_path or requested_path.startswith(('/','\\','~')):
        logger.warning(f"Directory traversal attempt detected: {requested_path}")
        return None
    
    try:
        # Resolve both paths to absolute paths
        base_resolved = base_dir.resolve()
        requested_resolved = (base_dir / requested_path).resolve()
        
        # Check if requested path is within base directory
        requested_resolved.relative_to(base_resolved)
        
        # Additional check: ensure it's a file, not a directory
        if requested_resolved.exists() and requested_resolved.is_dir():
            logger.warning(f"Attempted to access directory as file: {requested_resolved}")
            return None
        
        return requested_resolved
    except (ValueError, Exception) as e:
        logger.warning(f"Path validation failed for {requested_path}: {e}")
        return None

def validate_url(url: str) -> bool:
    """
    Validate URL to prevent SSRF attacks.
    Only allow http:// and https:// schemes and block private IP ranges.
    
    Args:
        url: URL string to validate
        
    Returns:
        True if URL is safe, False otherwise
    """
    if not url or not isinstance(url, str):
        return False
    
    url = url.strip()
    
    # Basic length check
    if len(url) > 4096:
        return False
    
    try:
        from urllib.parse import urlparse
        import ipaddress
        
        parsed = urlparse(url)
        
        # Only allow http and https schemes
        if parsed.scheme not in ('http', 'https'):
            return False
        
        # Check for empty hostname
        if not parsed.hostname:
            return False
        
        # Block localhost and private IP ranges
        hostname = parsed.hostname.lower()
        
        # Block localhost variations
        localhost_patterns = ['localhost', '127.', '0.0.0.0', '::1', '0:0:0:0:0:0:0:1', '[::1]']
        if any(hostname.startswith(pattern) for pattern in localhost_patterns):
            logger.warning(f"Blocked localhost access attempt: {hostname}")
            return False
        
        # Try to resolve as IP address and check if it's private
        try:
            ip = ipaddress.ip_address(hostname)
            if ip.is_private or ip.is_loopback or ip.is_reserved or ip.is_link_local:
                logger.warning(f"Blocked private IP access attempt: {ip}")
                return False
        except ValueError:
            # Not an IP address, check domain patterns
            pass
        
        # Block internal domains
        internal_domains = ['.local', '.internal', '.corp', '.lan', '.localdomain', '.test']
        if any(hostname.endswith(domain) for domain in internal_domains):
            logger.warning(f"Blocked internal domain access attempt: {hostname}")
            return False
        
        # Block common internal IP ranges and hostnames
        blocked_hosts = ['metadata.google.internal', 'kubernetes.default', 'consul.service']
        if hostname in blocked_hosts:
            logger.warning(f"Blocked known internal hostname: {hostname}")
            return False
        
        # Additional check: disallow URLs with credentials
        if parsed.username or parsed.password:
            logger.warning(f"Blocked URL with embedded credentials")
            return False
        
        return True
    except Exception as e:
        logger.error(f"URL validation error: {e}")
        return False


def sanitize_output_name(name: str) -> str:
    """Sanitize a user-provided output filename.

    - Removes path separators and control characters
    - Replaces Windows-reserved characters with underscore
    - Trims trailing dots/spaces and limits length to 255
    - Ensures the result is non-empty
    """
    try:
        import re
        import os

        if not name or not isinstance(name, str):
            return ''

        # Remove null bytes and control characters
        name = ''.join(ch for ch in name if ord(ch) >= 32 and ch != '\x00')

        # Replace path separators and traversal attempts
        name = name.replace('/', '_').replace('\\', '_')
        name = name.replace('..', '_')

        # Replace reserved characters
        name = re.sub(r'[<>:\"|\?\*]', '_', name)

        # Strip surrounding whitespace and dots (Windows doesn't allow trailing dots/spaces)
        name = name.strip().strip('.')

        # Enforce max length
        if len(name) > 255:
            name = name[:255]

        # Avoid reserved Windows device names
        reserved = {"CON","PRN","AUX","NUL"} | {f"COM{i}" for i in range(1,10)} | {f"LPT{i}" for i in range(1,10)}
        base = os.path.splitext(name)[0]
        if base.upper() in reserved:
            # Append underscore to avoid reserved name
            ext = os.path.splitext(name)[1]
            name = base + '_' + ext

        if not name:
            return 'download'

        return name
    except Exception:
        return 'download'


@app.route('/api/thumbnail', methods=['GET'])
def proxy_thumbnail():
    """Proxy a remote thumbnail via localhost to avoid browser tracking protection blocks.

    Query params:
      - url: remote image URL
    """
    try:
        raw_url = (request.args.get('url') or '').strip()
        insecure_ssl = (request.args.get('insecure_ssl') or '').strip() in ('1', 'true', 'True', 'yes', 'on')

        # If the caller didn't URL-encode the remote URL, Werkzeug will split
        # it at '&' and we only receive a truncated value. Reconstruct from
        # the raw query string by taking everything after 'url='.
        try:
            if raw_url and request.query_string:
                qs = request.query_string.decode('utf-8', errors='ignore')
                if 'url=' in qs:
                    after = qs.split('url=', 1)[1]
                    # If there are additional query parameters, they are
                    # likely part of the remote URL (unencoded '&').
                    if '&' in after and '&' not in raw_url:
                        from urllib.parse import unquote_plus
                        raw_url = unquote_plus(after).strip()
        except Exception:
            pass

        if not raw_url:
            return jsonify({'error': 'url is required'}), 400

        # Basic length guard
        if len(raw_url) > 4096:
            return jsonify({'error': 'url too long'}), 400

        if not validate_url(raw_url):
            return jsonify({'error': 'Invalid or unsafe URL'}), 400

        from urllib.request import Request, urlopen
        from urllib.error import URLError, HTTPError
        import ssl
        from urllib.parse import urlparse

        parsed = urlparse(raw_url)
        referer = 'https://vk.com/'
        host = (parsed.hostname or '').lower()
        if host:
            # VK thumbnails are often hosted on *.userapi.com but require vk.com referer.
            if host.endswith('userapi.com') or host.endswith('vkuserapi.com') or 'vk.com' in host:
                referer = 'https://vk.com/'
            elif 'rutube' in host:
                referer = 'https://rutube.ru/'
            elif parsed.scheme:
                referer = f"{parsed.scheme}://{host}/"

        req = Request(
            raw_url,
            headers={
                'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)',
                'Accept': 'image/avif,image/webp,image/apng,image/*,*/*;q=0.8',
                'Accept-Language': 'en-US,en;q=0.9',
                'Referer': referer,
                'Connection': 'keep-alive',
            },
        )

        # Hard limits to avoid huge responses
        max_bytes = 8 * 1024 * 1024  # 8MB

        def _fetch(context):
            with urlopen(req, timeout=15, context=context) as resp:
                content_type = resp.headers.get('Content-Type', 'image/jpeg')
                data = resp.read(max_bytes + 1)
                return content_type, data

        ctx = None
        if insecure_ssl:
            try:
                ctx = ssl._create_unverified_context()
            except Exception:
                ctx = None

        try:
            content_type, data = _fetch(ctx)
        except Exception as e:
            # urllib can raise ssl.SSLError directly, or wrap it in URLError.
            err_str = str(e)
            is_cert_verify = (
                'CERTIFICATE_VERIFY_FAILED' in err_str
                or 'certificate verify failed' in err_str.lower()
            )

            # Retry once without verification if not already using unverified context.
            if is_cert_verify and not insecure_ssl:
                try:
                    content_type, data = _fetch(ssl._create_unverified_context())
                except Exception as e2:
                    logger.warning(f"SSL retry failed for thumbnail: {raw_url} - {e2}")
                    raise
            else:
                # Propagate; outer handler will format response
                raise

        if len(data) > max_bytes:
            return jsonify({'error': 'Image too large'}), 413

        out = make_response(data)
        out.headers['Content-Type'] = content_type
        out.headers['Cache-Control'] = 'public, max-age=3600'
        out.headers['X-Content-Type-Options'] = 'nosniff'
        out.headers['X-Frame-Options'] = 'DENY'
        out.headers['Referrer-Policy'] = 'no-referrer'
        return out

    except (HTTPError, URLError) as e:
        # If SSL verify failed and caller didn't request insecure_ssl, tell them
        # (and we also retry internally once; this is for the remaining failure).
        err_str = str(e)
        if 'CERTIFICATE_VERIFY_FAILED' in err_str or 'certificate verify failed' in err_str.lower():
            logger.warning(f"SSL verification failed for thumbnail: {raw_url} - {e}")
            return jsonify({
                'error': f'SSL certificate verification failed while fetching thumbnail: {e}',
                'url': raw_url,
                'hint': 'Enable Insecure SSL in UI or append &insecure_ssl=1'
            }), 502

        logger.error(f"Failed to fetch thumbnail: {raw_url} - {e}")
        return jsonify({'error': f'Failed to fetch thumbnail: {e}', 'url': raw_url}), 502
    except Exception as e:
        logger.error(f"Unexpected error fetching thumbnail: {raw_url} - {e}", exc_info=True)
        return jsonify({'error': f'Unexpected error: {e}', 'url': raw_url}), 500

# Cancel download endpoint
@app.route('/api/cancel_download', methods=['POST'])
def cancel_download():
    """Cancel an active download by ID."""
    data = request.get_json()
    download_id = data.get('download_id') if data else None
    if not download_id or download_id not in active_downloads:
        return jsonify({'error': 'Invalid or missing download_id'}), 400
    active_downloads[download_id]['cancelled'] = True
    return jsonify({'status': 'cancelled'})

@app.route('/')
def index():
    """Serve the main page."""
    return render_template('index.html')

@app.route('/api/video_info', methods=['POST'])
@rate_limit(max_requests=15, window_seconds=60)
def get_video_info():
    """Get video information efficiently with timeout handling."""

    def timeout_handler(signum, frame):
        raise TimeoutError("Video info request timed out")

    try:
        data = request.get_json()
        if not data or 'url' not in data:
            resp = make_response(json.dumps({'error': 'URL is required'}), 400)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp
        url = data['url']
        if not url:
            resp = make_response(json.dumps({'error': 'URL is required'}), 400)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp
        url = url.strip()
        if not url:
            resp = make_response(json.dumps({'error': 'URL cannot be empty'}), 400)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp
        
        # Validate URL to prevent SSRF attacks
        if not validate_url(url):
            resp = make_response(json.dumps({'error': 'Invalid or unsafe URL'}), 400)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp

        # Check cache first for performance
        cached_info = _get_cached_video_info(url)
        if cached_info:
            resp = make_response(json.dumps(cached_info), 200)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            resp.headers['X-Cache'] = 'HIT'
            return resp

        # Allow client to request insecure SSL (skip certificate verification)
        insecure_ssl = bool(data.get('insecure_ssl')) if data else False

        # Use threading instead of signal for cross-platform timeout
        info_result = [None]
        error_result = [None]
        
        def get_info_thread():
            try:
                        # Temporarily set insecure flag for this downloader
                prev_insecure = getattr(downloader, 'insecure_ssl', False)
                try:
                    downloader.insecure_ssl = insecure_ssl
                    info_result[0] = downloader.get_video_info(url)
                finally:
                    downloader.insecure_ssl = prev_insecure
            except Exception as e:
                error_result[0] = str(e)
        
        thread = threading.Thread(target=get_info_thread, daemon=True)
        thread.start()
        thread.join(timeout=45)  # 45 second timeout
        
        if thread.is_alive():
            resp = make_response(json.dumps({'error': 'Request timed out. Please try again.'}), 408)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp
        
        if error_result[0]:
            resp = make_response(json.dumps({'error': f'Error: {error_result[0]}'}), 500)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp
        
        info = info_result[0]
        if not info:
            resp = make_response(json.dumps({'error': 'Could not retrieve video information'}), 400)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp
            
        # Resolve best thumbnail (fallback to largest from "thumbnails" list)
        def _pick_best_thumbnail(video_data: Dict[str, Any]) -> str:
            direct = video_data.get('thumbnail')
            if direct:
                return direct

            thumbs = video_data.get('thumbnails') or []
            try:
                # Prefer the largest thumbnail by area if width/height provided
                best = max(
                    thumbs,
                    key=lambda t: (t or {}).get('width', 0) * (t or {}).get('height', 0),
                )
                if isinstance(best, dict):
                    return best.get('url') or best.get('src') or ''
            except Exception:
                pass
            # Fallback: return first available url/src
            for t in thumbs:
                if isinstance(t, dict):
                    url = t.get('url') or t.get('src')
                    if url:
                        return url
                elif isinstance(t, str):
                    return t
            return ''

        # Extract and optimize video information
        video_info = {
            'title': info.get('title', 'Unknown'),
            'duration': info.get('duration_string', 'Unknown'),
            'uploader': info.get('uploader', 'Unknown'),
            'view_count': info.get('view_count', 0),
            'thumbnail': _pick_best_thumbnail(info),
        }
        # Extract available qualities efficiently with improved resolution detection
        formats = info.get('formats', [])
        qualities = set()
        
        # Track actual resolutions found to avoid duplicates
        found_resolutions = set()
        
        # Track available audio languages
        audio_languages = {}  # {language_code: language_name}

        def _extract_height(fmt: Dict[str, Any]) -> Optional[int]:
            """Best-effort resolution detection from various fields."""
            if fmt.get('height') and isinstance(fmt.get('height'), int):
                return fmt['height']
            # Resolution string like "3840x2160"
            res = fmt.get('resolution') or fmt.get('res')
            if isinstance(res, str) and 'x' in res:
                try:
                    parts = res.lower().split('x')
                    if len(parts) == 2:
                        return int(parts[1])
                except Exception:
                    pass
            # format_note like "2160p" or "1080p50"
            note = fmt.get('format_note') or fmt.get('quality')
            if isinstance(note, str):
                import re
                m = re.search(r'(\d{3,4})p', note)
                if m:
                    try:
                        return int(m.group(1))
                    except ValueError:
                        pass
            return None

        def _quality_label(height: int) -> Tuple[str, int]:
            if height >= 2000:
                return '4k', 2160
            if height >= 1350:
                return '1440p', 1440
            if height >= 1000:
                return '1080p', 1080
            if height >= 650:
                return '720p', 720
            if height >= 420:
                return '480p', 480
            if height >= 300:
                return '360p', 360
            if height >= 200:
                return '240p', 240
            if height >= 100:
                return '144p', 144
            return '', 0

        for fmt in formats:
            height = _extract_height(fmt)
            if height:
                label, canonical_height = _quality_label(height)
                if label:
                    qualities.add(label)
                    found_resolutions.add(canonical_height)
            
            # Extract audio language information
            if fmt.get('acodec') and fmt.get('acodec') != 'none':
                lang_code = fmt.get('language') or fmt.get('lang') or 'unknown'
                lang_name = fmt.get('language_name') or lang_code
                if lang_code and lang_code != 'unknown':
                    audio_languages[lang_code] = lang_name
        
        # Log available formats for troubleshooting (optional, can be disabled in production)
        if config.DEBUG_MODE:
            print(f"Found resolutions: {sorted(found_resolutions, reverse=True)}")
            print(f"Available qualities: {sorted(qualities, key=lambda x: {'4k': 2160, '1440p': 1440, '1080p': 1080, '720p': 720, '480p': 480, '360p': 360, '240p': 240, '144p': 144}.get(x, 0), reverse=True)}")
            print(f"Available audio languages: {audio_languages}")
        
        # Sort qualities by resolution
        quality_order = {'4k': 2160, '1440p': 1440, '1080p': 1080, '720p': 720, '480p': 480, '360p': 360, '240p': 240, '144p': 144}
        video_info['available_qualities'] = sorted(
            qualities, 
            key=lambda x: quality_order.get(x, 0), 
            reverse=True
        )
        
        # Add available audio languages to response
        video_info['available_audio_languages'] = [
            {'code': code, 'name': name} 
            for code, name in sorted(audio_languages.items())
        ]
        
        # Log available formats for troubleshooting if debug mode enabled
        if config.DEBUG_MODE:
            downloader.debug_available_formats(url)

        # Cache the result for future requests
        _cache_video_info(url, video_info)
        
        resp = make_response(json.dumps(video_info), 200)
        resp.headers['Content-Type'] = 'application/json; charset=utf-8'
        return resp
    except Exception as e:
        resp = make_response(json.dumps({'error': f'Server error: {str(e)}'}), 500)
        resp.headers['Content-Type'] = 'application/json; charset=utf-8'
        return resp

@app.route('/api/download', methods=['POST'])
@rate_limit(max_requests=5, window_seconds=60)
def start_download():
    """Start video download with optimized error handling."""
    try:
        data = request.get_json()
        if not data or 'url' not in data:
            resp = make_response(json.dumps({'error': 'URL is required'}), 400)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp

        url = (data.get('url') or '').strip()
        if not url:
            resp = make_response(json.dumps({'error': 'URL cannot be empty'}), 400)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp
        
        # Validate URL to prevent SSRF attacks
        if not validate_url(url):
            resp = make_response(json.dumps({'error': 'Invalid or unsafe URL'}), 400)
            resp.headers['Content-Type'] = 'application/json; charset=utf-8'
            return resp

        quality = data.get('quality', 'best')
        audio_only = data.get('audio_only', False)
        audio_language = data.get('audio_language')
        output_name = (data.get('output_name') or '').strip() or None

        # Sanitize output_name instead of rejecting it to improve UX.
        # We still prevent path traversal and reserved names but accept and sanitize common filenames.
        output_name_notice = None
        if output_name:
            sanitized = sanitize_output_name(output_name)
            if not sanitized:
                resp = make_response(json.dumps({'error': 'Invalid output filename'}), 400)
                resp.headers['Content-Type'] = 'application/json; charset=utf-8'
                return resp
            if sanitized != output_name:
                output_name_notice = f"Requested filename sanitized to '{sanitized}'"
                output_name = sanitized
        
        insecure_ssl = bool(data.get('insecure_ssl'))
        trim_start = data.get('trim_start')
        trim_end = data.get('trim_end')

        if config.DEBUG_MODE:
            print(f"Received output_name for download: {output_name}")
            print(f"Received audio_language for download: {audio_language}")
            if trim_start is not None or trim_end is not None:
                print(f"Trim parameters: start={trim_start}, end={trim_end}")

        download_id = str(uuid.uuid4())

        def download_task():
            prev_insecure = getattr(downloader, 'insecure_ssl', False)
            try:
                downloader.insecure_ssl = insecure_ssl
                downloader.audio_only = audio_only
                downloader.audio_language = audio_language
                downloader.trim_start = trim_start
                downloader.trim_end = trim_end
                downloader.set_download_id(download_id)

                if download_id in active_downloads:
                    state = active_downloads[download_id]
                    state['requested_quality'] = quality
                    state['insecure_ssl'] = insecure_ssl

                    notices = state.get('download_notice')
                    if insecure_ssl:
                        warning = 'SSL verification disabled for this download.'
                        notices = f"{notices} {warning}".strip() if notices else warning

                    # If we sanitized the output filename, show a notice to the user
                    if output_name_notice:
                        notices = f"{notices} {output_name_notice}".strip() if notices else output_name_notice

                    if not downloader.merger.ffmpeg_available and quality.lower() not in ['360p', 'best']:
                        extra_notice = (
                            f'Requested {quality}, but only 360p available due to no FFmpeg. '
                            'Install FFmpeg for higher quality downloads.'
                        )
                        notices = f"{notices} {extra_notice}".strip() if notices else extra_notice

                    if notices:
                        state['download_notice'] = notices

                success = downloader.download_video(url, quality, audio_only, output_name)
                if not success and download_id in active_downloads:
                    active_downloads[download_id]['status'] = 'error'
                    active_downloads[download_id]['error'] = 'Download failed'
            except Exception as e:
                if download_id in active_downloads:
                    active_downloads[download_id]['status'] = 'error'
                    active_downloads[download_id]['error'] = str(e)
            finally:
                downloader.insecure_ssl = prev_insecure

        thread = threading.Thread(target=download_task, daemon=True)
        thread.start()

        resp = make_response(json.dumps({'download_id': download_id, 'insecure_ssl': insecure_ssl}), 200)
        resp.headers['Content-Type'] = 'application/json; charset=utf-8'
        return resp
    except Exception as e:
        resp = make_response(json.dumps({'error': f'Server error: {str(e)}'}), 500)
        resp.headers['Content-Type'] = 'application/json; charset=utf-8'
        return resp


@app.route('/api/progress/<download_id>')
def get_progress(download_id: str):
    """Get download progress efficiently."""
    try:
        if download_id in active_downloads:
            progress = active_downloads[download_id].copy()
        elif download_id in completed_downloads:
            progress = completed_downloads[download_id].copy()
        else:
            return jsonify({'error': 'Download not found'}), 404

        progress.setdefault('download_id', download_id)
        return jsonify(progress)
    except Exception as e:
        return jsonify({'error': f'Progress error: {str(e)}'}), 500


@app.route('/api/download/<download_id>/file', methods=['GET', 'HEAD', 'OPTIONS'])
def download_file(download_id: str):
    """Download completed file by ID."""
    try:
        if request.method == 'OPTIONS':
            response = make_response('', 204)
            response.headers['Access-Control-Allow-Origin'] = '*'
            response.headers['Access-Control-Allow-Methods'] = 'GET, HEAD, OPTIONS'
            response.headers['Access-Control-Allow-Headers'] = 'Content-Type'
            return response

        file_info = completed_downloads.get(download_id) or active_downloads.get(download_id)
        file_path = file_info.get('file_path') if file_info else None

        # If file_path is present but missing on disk, or not provided at all,
        # try to map the download id / filename to an actual file in the downloads directory.
        downloads_dir = Path('./downloads').resolve()

        def persist_found_path(found_path: Path):
            file_path = str(found_path)
            # Persist for future requests
            try:
                if download_id in completed_downloads:
                    completed_downloads[download_id]['file_path'] = file_path
                elif download_id in active_downloads:
                    active_downloads[download_id]['file_path'] = file_path
            except Exception:
                pass

        if file_path:
            try:
                p = Path(file_path).resolve()
                # Ensure it's inside our downloads dir
                try:
                    p.relative_to(downloads_dir)
                except ValueError:
                    # Not inside downloads dir - treat as missing for safety
                    p = None
                if not p or not p.exists():
                    file_path = None
                else:
                    file_path = str(p)
            except Exception:
                file_path = None

        # Try to locate by filename or download id if we don't have a valid file_path yet
        if (not file_path) and downloads_dir.exists():
            expected_names = set()
            if file_info:
                name = file_info.get('filename')
                if name:
                    expected_names.add(name)
                    expected_names.add(name.lower())

            # Walk the downloads directory for a best match
            for candidate in downloads_dir.glob('*'):
                if not candidate.is_file():
                    continue

                candidate_name = candidate.name
                match = False
                if expected_names:
                    if candidate_name in expected_names or candidate_name.lower() in expected_names:
                        match = True
                else:
                    if download_id in candidate_name:
                        match = True

                # Also accept case-insensitive or partial matches if nothing exact found
                if not match and expected_names:
                    for en in expected_names:
                        if en and (en in candidate_name or candidate_name in en or candidate_name.lower() == en.lower()):
                            match = True
                            break

                if match:
                    persist_found_path(candidate.resolve())
                    break

        if not file_path or not os.path.exists(file_path):
            return jsonify({'error': 'File not found or no longer available'}), 404
        
        # Check file size for security (prevent serving extremely large files)
        try:
            file_size = os.path.getsize(file_path)
            max_file_size = 10 * 1024 * 1024 * 1024  # 10GB limit
            if file_size > max_file_size:
                return jsonify({'error': 'File too large to download'}), 413
        except OSError as size_err:
            print(f"Error checking file size: {size_err}")
            return jsonify({'error': 'Error accessing file'}), 500
        
        if request.method == 'HEAD':
            response = make_response('', 200)
        else:
            filename = os.path.basename(file_path)
            if isinstance(filename, bytes):
                filename = filename.decode('utf-8', errors='ignore')
            
            # Sanitize filename to ASCII for HTTP headers (prevents UnicodeEncodeError)
            safe_filename = filename.encode('ascii', 'ignore').decode('ascii')
            if not safe_filename:
                safe_filename = 'download.mp4'
            
            try:
                response = send_file(file_path, as_attachment=True, download_name=safe_filename)
            except TypeError:
                response = send_file(file_path, as_attachment=True, attachment_filename=safe_filename)
            
            # Add UTF-8 filename in RFC 2231 format for modern browsers
            from urllib.parse import quote
            response.headers['Content-Disposition'] = f"attachment; filename={safe_filename}; filename*=UTF-8''{quote(filename)}"

        notice = file_info.get('download_notice') if file_info else None
        if notice:
            response.headers['X-Download-Notice'] = notice

        response.headers['X-Content-Type-Options'] = 'nosniff'
        response.headers['X-Frame-Options'] = 'DENY'
        response.headers['Referrer-Policy'] = 'no-referrer'
        response.headers['Access-Control-Allow-Origin'] = '*'
        response.headers['Access-Control-Allow-Methods'] = 'GET, HEAD, OPTIONS'
        response.headers['Access-Control-Allow-Headers'] = 'Content-Type'
        return response
    except Exception as e:
        logger.error(f"Download error for {download_id}: {str(e)}", exc_info=True)
        return jsonify({'error': f'Unexpected error: {str(e)}'}), 500


@app.route('/download_by_filename/<path:filename>', methods=['GET', 'HEAD'])
def download_by_filename(filename: str):
    """Download file by filename - backup method when download ID is not available."""
    try:
        # Sanitize filename to prevent directory traversal
        safe_filename = os.path.basename(filename)
        
        # Handle URL encoding/decoding
        try:
            from urllib.parse import unquote
            safe_filename = unquote(safe_filename)
        except Exception:
            pass
        
        # Validate filename doesn't contain path separators
        if os.path.sep in safe_filename or (os.path.altsep and os.path.altsep in safe_filename):
            return jsonify({'error': 'Invalid filename'}), 400
        
        # Additional validation - reject suspicious patterns
        if '..' in safe_filename or safe_filename.startswith('.'):
            return jsonify({'error': 'Invalid filename'}), 400
        
        # Reject filenames with null bytes or control characters
        if '\x00' in safe_filename or any(ord(c) < 32 for c in safe_filename):
            return jsonify({'error': 'Invalid filename'}), 400
        
        downloads_dir = Path('./downloads').resolve()
        file_path = validate_safe_path(safe_filename, downloads_dir)
        
        if not file_path or not file_path.exists() or not file_path.is_file():
            # Try to find files that might match closely
            if downloads_dir.exists():
                for existing_file in downloads_dir.glob('*'):
                    if existing_file.is_file():
                        existing_name = existing_file.name
                        # Try exact match first
                        if existing_name == safe_filename:
                            file_path = existing_file
                            break
                        # Try case-insensitive match
                        elif existing_name.lower() == safe_filename.lower():
                            file_path = existing_file
                            break
                        # Try partial match (useful for files with special characters)
                        elif safe_filename in existing_name or existing_name in safe_filename:
                            file_path = existing_file
                            break
        
        if not file_path or not file_path.exists() or not file_path.is_file():
            return jsonify({'error': 'File not found'}), 404 if request.method == 'GET' else ('', 404)
        
        # Double-check the file is still within downloads directory
        try:
            file_path.relative_to(downloads_dir)
        except ValueError:
            return jsonify({'error': 'Access denied'}), 403
        
        # Handle HEAD request - just check if file exists
        if request.method == 'HEAD':
            return '', 200
        
        # Handle GET request - actual file download
        try:
            filename_to_send = file_path.name
            # Ensure filename is properly encoded
            if isinstance(filename_to_send, bytes):
                filename_to_send = filename_to_send.decode('utf-8', errors='ignore')
            
            # Sanitize filename to ASCII for HTTP headers (prevents UnicodeEncodeError)
            safe_filename = filename_to_send.encode('ascii', 'ignore').decode('ascii')
            if not safe_filename:
                safe_filename = 'download.mp4'
            
            try:
                # Try newer Flask version parameter first
                response = send_file(str(file_path), as_attachment=True, download_name=safe_filename)
            except TypeError:
                # Fallback to older Flask version parameter
                response = send_file(str(file_path), as_attachment=True, attachment_filename=safe_filename)
            
            # Add UTF-8 filename in RFC 2231 format for modern browsers
            from urllib.parse import quote
            response.headers['Content-Disposition'] = f"attachment; filename={safe_filename}; filename*=UTF-8''{quote(filename_to_send)}"
            
            # Add security headers
            response.headers['X-Content-Type-Options'] = 'nosniff'
            response.headers['X-Frame-Options'] = 'DENY'
            response.headers['Referrer-Policy'] = 'no-referrer'
            # Add CORS headers for better compatibility
            response.headers['Access-Control-Allow-Origin'] = '*'
            response.headers['Access-Control-Allow-Methods'] = 'GET, HEAD, OPTIONS'
            response.headers['Access-Control-Allow-Headers'] = 'Content-Type'
            
            return response
        except Exception as e:
            print(f"Error sending file {file_path}: {str(e)}")
            return jsonify({'error': f'Download error: {str(e)}'}), 500
    
    except Exception as e:
        print(f"General error in download_by_filename for {filename}: {str(e)}")
        return jsonify({'error': f'Download error: {str(e)}'}), 500

@app.route('/api/open_file', methods=['POST'])
def open_file():
    """Open a downloaded file locally using the system default video player."""
    try:
        data = request.get_json(force=True)
        if not data:
            return jsonify({'error': 'No data provided'}), 400
        
        file_path = data.get('file_path') or data.get('filePath')
        if not file_path:
            return jsonify({'error': 'file_path is required'}), 400
        
        # Sanitize and validate path
        downloads_dir = Path('./downloads').resolve()
        
        # Handle both absolute and relative paths
        if os.path.isabs(file_path):
            target_path = Path(file_path).resolve()
        else:
            target_path = (downloads_dir / file_path).resolve()
        
        # Validate path is within downloads directory (prevent path traversal)
        try:
            target_path.relative_to(downloads_dir)
        except ValueError:
            print(f"Security: Attempted access outside downloads directory: {file_path}")
            return jsonify({'error': 'File outside downloads directory is not allowed'}), 403

        if not target_path.exists() or not target_path.is_file():
            return jsonify({'error': 'File not found'}), 404

        system = platform.system()
        try:
            if system == 'Windows':
                # Use os.startfile which is safe on Windows
                os.startfile(str(target_path))  # type: ignore[attr-defined]
            elif system == 'Darwin':
                # Explicitly pass shell=False and use list of arguments
                subprocess.Popen(['open', str(target_path)], shell=False)
            else:  # Linux and other Unix-like systems
                # Explicitly pass shell=False and use list of arguments
                subprocess.Popen(['xdg-open', str(target_path)], shell=False)
        except Exception as open_err:
            print(f"Error opening file: {open_err}")
            return jsonify({'error': f'Unable to open file: {open_err}'}), 500

        return jsonify({'status': 'opened', 'file': target_path.name})

    except Exception as e:
        print(f"Unexpected error in open_file: {str(e)}")
        return jsonify({'error': f'Unexpected error: {str(e)}'}), 500

@app.route('/api/debug_formats', methods=['POST'])
def debug_formats():
    """Debug endpoint to show available formats for a video."""
    try:
        data = request.get_json()
        if not data or 'url' not in data:
            return jsonify({'error': 'URL is required'}), 400
        
        url = data['url'].strip()
        if not url:
            return jsonify({'error': 'URL cannot be empty'}), 400
        
        # Validate URL to prevent SSRF attacks
        if not validate_url(url):
            return jsonify({'error': 'Invalid or unsafe URL'}), 400
        
        # Get debug format information
        debug_info = downloader.debug_available_formats(url)
        
        return jsonify({
            'url': url,
            'debug_info': debug_info,
            'message': 'Debug information logged to console'
        })
    
    except Exception as e:
        return jsonify({'error': f'Debug error: {str(e)}'}), 500

@app.errorhandler(404)
def not_found(error):
    resp = make_response(json.dumps({'error': 'Not found'}), 404)
    resp.headers['Content-Type'] = 'application/json; charset=utf-8'
    return resp

@app.errorhandler(500)
def internal_error(error):
    resp = make_response(json.dumps({'error': 'Internal server error'}), 500)
    resp.headers['Content-Type'] = 'application/json; charset=utf-8'
    return resp


@app.route('/api/completed_downloads', methods=['GET'])
def list_completed_downloads():
    """Return a JSON list of completed downloads. Secured with a token.

    Use environment variable DOWNLOADS_API_TOKEN or app.config['DOWNLOADS_API_TOKEN']
    to protect this endpoint.
    """
    try:
        token = os.environ.get('DOWNLOADS_API_TOKEN') or app.config.get('DOWNLOADS_API_TOKEN')
        if token:
            req_token = request.headers.get('X-API-Token') or request.args.get('token')
            if not req_token or req_token != token:
                return jsonify({'error': 'Unauthorized'}), 401

        # Build a minimal safe listing
        results = []
        for did, info in completed_downloads.items():
            results.append({
                'download_id': did,
                'filename': info.get('filename'),
                'file_path': info.get('file_path'),
                'status': info.get('status', 'completed'),
                'downloaded_bytes': info.get('downloaded_bytes'),
                'total_bytes': info.get('total_bytes'),
                'timestamp': info.get('started_at') or info.get('timestamp')
            })

        return jsonify({'completed': results}), 200
    except Exception as e:
        print(f'Error listing completed downloads: {e}')
        return jsonify({'error': f'Internal error: {e}'}), 500

@app.after_request
def add_security_headers(response):
    # Add security and cache headers for all responses
    response.headers['X-Content-Type-Options'] = 'nosniff'
    response.headers['X-Frame-Options'] = 'DENY'
    response.headers['Referrer-Policy'] = 'no-referrer'
    
    # Add Content Security Policy to prevent XSS attacks
    # Allow Google Fonts and other CDN resources for better UI
    csp_policy = (
        "default-src 'self'; "
        "script-src 'self' 'unsafe-inline'; "
        "style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; "
        "img-src 'self' data: https:; "
        "font-src 'self' https://fonts.gstatic.com; "
        "connect-src 'self'; "
        "frame-ancestors 'none'; "
        "base-uri 'self'; "
        "form-action 'self'"
    )
    response.headers['Content-Security-Policy'] = csp_policy
    
    # Add Strict-Transport-Security for HTTPS (when deployed with HTTPS)
    # response.headers['Strict-Transport-Security'] = 'max-age=31536000; includeSubDomains'
    
    # Remove deprecated/undesired headers
    response.headers.pop('X-XSS-Protection', None)
    response.headers.pop('Expires', None)
    
    # Cache control for static files
    if request.path.startswith('/static/') or request.path.endswith('.js') or request.path.endswith('.css'):
        response.headers['Cache-Control'] = 'public, max-age=31536000, immutable'
    else:
        # Prevent caching of dynamic content
        response.headers['Cache-Control'] = 'no-store, no-cache, must-revalidate, max-age=0'
        response.headers['Pragma'] = 'no-cache'
    
    return response

if __name__ == '__main__':
    import platform
    
    # Ensure templates directory exists
    templates_dir = Path('templates')
    templates_dir.mkdir(exist_ok=True)
    
    # Ensure downloads directory exists
    downloads_dir = Path('downloads')
    downloads_dir.mkdir(exist_ok=True)
    
    # Platform information
    system_info = f"{platform.system()} {platform.release()}"
    python_version = f"{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}"
    
    print()
    # Print platform icon and info
    platform_icons = {
        'Windows': '🪟',
        'Linux': '🐧',
        'Darwin': '🍏',
        'macOS': '🍏'
    }
    icon = platform_icons.get(platform.system(), '💻')
    print(f"{icon} Platform: {system_info}")
    print(f"🐍 Python: {python_version}")
    print("🚀 Starting YTDL Web Interface...")
    print("📱 Open your browser and go to: http://localhost:5005")
    print("🛑 Press Ctrl+C to stop the server")
    if config.DEBUG_MODE:
        print("🔁 Press Ctrl+R to restart the server")
    
    use_waitress = os.environ.get('USE_WAITRESS', '').strip() == '1'

    try:
        if not use_waitress:
            raise ImportError('Waitress disabled (set USE_WAITRESS=1 to enable)')

        # Production server (Waitress) when explicitly enabled
        from waitress import serve
        if config.DEBUG_MODE:
            print("\U0001f3ed Using production server (Waitress)")

        # Start a background thread to watch for Ctrl+R in console (Windows)
        def restart_watcher():
            try:
                if os.name == 'nt':
                    import msvcrt
                    while True:
                        if msvcrt.kbhit():
                            ch = msvcrt.getch()
                            # Ctrl+R sends ASCII 18 (0x12)
                            if ch == b'\x12':
                                print('\n[INFO] Ctrl+R detected — restarting server...')
                                time.sleep(0.2)
                                os.execv(sys.executable, [sys.executable] + sys.argv)
                        time.sleep(0.1)
                else:
                    # For POSIX, listen for SIGUSR1 as restart signal
                    import signal
                    def _handler(signum, frame):
                        print('\n[INFO] Restart signal received — restarting server...')
                        os.execv(sys.executable, [sys.executable] + sys.argv)
                    signal.signal(signal.SIGUSR1, _handler)
            except Exception as e:
                print(f'[WARN] restart_watcher error: {e}')

        watcher = threading.Thread(target=restart_watcher, daemon=True)
        watcher.start()
        try:
            serve(app, host='0.0.0.0', port=5005, threads=6)
        except BaseException as e:
            print(f"[ERROR] Waitress failed to start ({type(e).__name__}): {e}")
            import traceback
            traceback.print_exc()
            print("⚠️ Falling back to Flask development server")
            app.run(
                debug=config.DEBUG_MODE,
                host='0.0.0.0',
                port=5005,
                threaded=True,
                use_reloader=False
            )
    except ImportError:
        # Fallback to Flask development server with optimized settings
        if config.DEBUG_MODE:
            print("\u26a0\ufe0f Using development server (NOT recommended for production)")
            print("\U0001f4a1 For production, install Waitress: pip install waitress")
            print("   Then run with: SET USE_WAITRESS=1 && python web_app.py")
            print()
        app.run(
            debug=config.DEBUG_MODE,
            host='0.0.0.0',
            port=5005,
            threaded=True,
            use_reloader=False
        )
