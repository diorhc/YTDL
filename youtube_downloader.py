#!/usr/bin/env python3
"""
YTDL - Optimized Solution

A robust YTDL with advanced features:
- Multiple quality options (144p to 8K)
- Audio-only downloads
- Error recovery and retry logic
- SSL certificate handling
- Multi-mode support (ultra/standard)
- FFmpeg and MoviePy integration
"""

import os
import sys
import argparse
import tempfile
import time
import concurrent.futures
import random
import platform
import re
import subprocess
import logging
from pathlib import Path
from typing import Optional, Dict, Any, Tuple, List

import yt_dlp
from colorama import init, Fore

try:
    import config
except ImportError:
    # Fallback if config.py is not available
    class config:
        DEBUG_MODE = False

# Initialize colorama for cross-platform colored output
init(autoreset=True)

# Configure logging based on DEBUG_MODE
logging.basicConfig(
    level=logging.DEBUG if config.DEBUG_MODE else logging.WARNING,
    format='%(asctime)s - %(name)s - %(levelname)s - %(message)s'
)
logger = logging.getLogger(__name__)

class ErrorHandler:
    """Error recovery system for streaming downloads."""
    
    @staticmethod
    def get_robust_options(base_opts: Dict[str, Any]) -> Dict[str, Any]:
        """Enhanced yt-dlp options for maximum success rate."""
        robust_opts = base_opts.copy()
        robust_opts.update({
            'http_headers': {
                'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36',
                'Accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
                'Accept-Language': 'en-us,en;q=0.5',
                'Accept-Encoding': 'gzip,deflate',
                'Accept-Charset': 'ISO-8859-1,utf-8;q=0.7,*;q=0.7',
                'Connection': 'keep-alive',
                'Upgrade-Insecure-Requests': '1',
            },
            'retries': 8,  # Increased retries for 403 errors
            'fragment_retries': 8,
            'retry_sleep_functions': {'http': lambda n: min(60, 2 ** n + random.uniform(0, 2))},
            'socket_timeout': 60,  # Increased timeout
            'http_chunk_size': 10485760,
            'geo_bypass': True,
            'geo_bypass_country': 'US',
            'extractor_args': {
                'youtube': {
                    'skip': ['hls', 'dash'],
                    'player_skip': ['configs'],
                    'innertube_host': ['studio.youtube.com', 'youtubei.googleapis.com'],
                }
            },
            # Additional 403 mitigation
            'writesubtitles': False,
            'writeautomaticsub': False,
            'ignoreerrors': False,
        })
        return robust_opts
    
    @staticmethod
    def get_fallback_user_agents():
        """Return list of user agents to try for 403 errors."""
        return [
            'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36',
            'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36',
            'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36',
            'Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:122.0) Gecko/20100101 Firefox/122.0',
            'Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/121.0.0.0 Safari/537.36',
        ]
    
    @staticmethod
    def handle_403_error(url: str, opts: Dict[str, Any], attempt: int) -> Dict[str, Any]:
        """Special handling for 403 Forbidden errors."""
        user_agents = ErrorHandler.get_fallback_user_agents()
        
        # Use different user agent for each attempt
        ua_index = attempt % len(user_agents)
        opts['http_headers']['User-Agent'] = user_agents[ua_index]
        
        # Add additional delay for 403 errors
        if attempt > 0:
            delay = min(30, 5 + attempt * 3 + random.uniform(1, 3))
            print(f"🛡️ 403 Recovery: Waiting {delay:.1f}s before retry {attempt + 1}")
            time.sleep(delay)
        
        # Additional 403-specific options
        opts.update({
            'socket_timeout': 90,
            'retries': 3,
            'fragment_retries': 3,
        })
        
        return opts

class VideoMerger:
    """Pure Python video/audio merger using MoviePy."""
    
    def __init__(self):
        self.available = self._check_moviepy()
        self.ffmpeg_path = 'ffmpeg'  # Default to system PATH
        self.ffmpeg_available = self._check_ffmpeg()
        
    def _check_moviepy(self) -> bool:
        """Check MoviePy availability."""
        try:
            from moviepy.video.io.VideoFileClip import VideoFileClip
            from moviepy.audio.io.AudioFileClip import AudioFileClip
            return True
        except ImportError:
            return False
    
    def _check_ffmpeg(self) -> bool:
        """Check FFmpeg availability in PATH and embedded Python directory."""
        
        # First try system PATH
        try:
            result = subprocess.run(['ffmpeg', '-version'], 
                                  capture_output=True, text=True, encoding='utf-8', timeout=5)
            if result.returncode == 0:
                return True
        except (subprocess.TimeoutExpired, FileNotFoundError, OSError):
            pass
        
        # Check embedded Python directory (Windows)
        try:
            embedded_ffmpeg = os.path.join(os.path.dirname(__file__), 'python_embedded', 'bin', 'ffmpeg.exe')
            if os.path.exists(embedded_ffmpeg):
                result = subprocess.run([embedded_ffmpeg, '-version'], 
                                      capture_output=True, text=True, encoding='utf-8', timeout=5)
                if result.returncode == 0:
                    # Store the path for later use
                    self.ffmpeg_path = embedded_ffmpeg
                    return True
        except (subprocess.TimeoutExpired, FileNotFoundError, OSError):
            pass
        
        # Check common installation paths for Linux/macOS
        system = platform.system()
        
        if system == 'Linux':
            linux_paths = [
                '/usr/bin/ffmpeg',
                '/usr/local/bin/ffmpeg',
                '/snap/bin/ffmpeg',
                '/opt/homebrew/bin/ffmpeg'  # If using Homebrew on Linux
            ]
            for path in linux_paths:
                if os.path.exists(path):
                    try:
                        result = subprocess.run([path, '-version'], 
                                              capture_output=True, text=True, encoding='utf-8', timeout=5)
                        if result.returncode == 0:
                            self.ffmpeg_path = path
                            return True
                    except (subprocess.TimeoutExpired, FileNotFoundError, OSError):
                        continue
        
        elif system == 'Darwin':  # macOS
            macos_paths = [
                '/usr/local/bin/ffmpeg',
                '/opt/homebrew/bin/ffmpeg',  # Apple Silicon Homebrew
                '/usr/bin/ffmpeg',
                '/Applications/ffmpeg'
            ]
            for path in macos_paths:
                if os.path.exists(path):
                    try:
                        result = subprocess.run([path, '-version'], 
                                              capture_output=True, text=True, encoding='utf-8', timeout=5)
                        if result.returncode == 0:
                            self.ffmpeg_path = path
                            return True
                    except (subprocess.TimeoutExpired, FileNotFoundError, OSError):
                        continue
            
        return False
    
    def merge_streams(self, video_path: str, audio_path: str, output_path: str) -> bool:
        """Merge video and audio streams using pure Python."""
        if not self.available:
            print(f"{Fore.RED}❌ MoviePy not available for merging")
            return False
            
        try:
            from moviepy.video.io.VideoFileClip import VideoFileClip
            from moviepy.audio.io.AudioFileClip import AudioFileClip
            
            if config.DEBUG_MODE:
                print(f"{Fore.CYAN}🔄 Merging streams with MoviePy...")
            
            # Load and combine
            video_clip = VideoFileClip(video_path)
            audio_clip = AudioFileClip(audio_path)
            final_clip = video_clip.set_audio(audio_clip)
            
            # Export with optimization
            final_clip.write_videofile(
                output_path,
                codec='libx264',
                audio_codec='aac',
                verbose=False,
                logger=None
            )
            
            # Cleanup
            video_clip.close()
            audio_clip.close() 
            final_clip.close()
            
            print(f"{Fore.GREEN}✅ Merge completed successfully")
            return True
            
        except Exception as e:
            print(f"{Fore.RED}❌ Merge failed: {e}")
            return False

class YouTubeDownloader:
    """
    Ultimate YTDL combining all best practices.
    Supports both standard and ultra modes with intelligent fallbacks.
    """
    
    def __init__(self, download_path: str = "./downloads", insecure_ssl: bool = False):
        self.download_path = Path(download_path)
        self.download_path.mkdir(exist_ok=True)
        self.merger = VideoMerger()
        self.error_handler = ErrorHandler()
        self.progress_hook_callback = None
        self.audio_language = None  # Selected audio language
        self.trim_start = None  # Trim start time in seconds
        self.trim_end = None  # Trim end time in seconds
        # If true, pass nocheckcertificate=True to yt-dlp options (insecure)
        self.insecure_ssl = bool(insecure_ssl)
    
    def _is_cancelled(self):
        """Check if the current download has been cancelled (for web interface)."""
        try:
            from web_app import active_downloads
            if hasattr(self, 'download_id') and self.download_id in active_downloads:
                return active_downloads[self.download_id].get('cancelled', False)
        except Exception:
            pass
        return False
        
    def set_progress_hook(self, callback):
        """Set progress hook callback for web interface compatibility."""
        self.progress_hook_callback = callback
    
    def _apply_ssl_options(self, opts: Dict[str, Any]) -> Dict[str, Any]:
        """Apply SSL options based on insecure_ssl flag."""
        if self.insecure_ssl:
            opts['nocheckcertificate'] = True
        return opts

    def _inject_ffmpeg_location(self, opts: Dict[str, Any]) -> Dict[str, Any]:
        """Ensure yt-dlp sees the bundled FFmpeg, preventing postprocess failures."""
        try:
            if self.merger.ffmpeg_available and getattr(self.merger, 'ffmpeg_path', None):
                merged = dict(opts)
                merged.setdefault('ffmpeg_location', self.merger.ffmpeg_path)
                return merged
        except Exception:
            pass
        return opts
    
    def _is_ssl_error(self, error_str: str) -> bool:
        """Check if error is an SSL certificate verification error."""
        ssl_indicators = [
            'CERTIFICATE_VERIFY_FAILED', 'certificate verify failed', 
            'CertificateVerifyError', '[SSL:', 'SSL: CERTIFICATE_VERIFY_FAILED'
        ]
        return any(indicator.lower() in error_str.lower() for indicator in ssl_indicators)
        
    def _progress_hook(self, d: Dict[str, Any]) -> None:
        """Internal progress hook that calls external callback if set, and aborts if cancelled."""
        # Abort download if cancelled
        if self._is_cancelled():
            raise Exception("Download cancelled by user")
        # When a file is finished, validate container metadata and fix if needed
        try:
            if d.get('status') in ('finished', 'done') and d.get('filename'):
                try:
                    fixed = self._validate_and_fix_file(d['filename'])
                    if fixed and fixed != d['filename']:
                        # Update filename in the progress dict so callers see corrected path
                        d = dict(d)
                        d['filename'] = fixed
                    
                    # Apply trim if requested - but ONLY to final merged files, not intermediate streams
                    if self.trim_start is not None or self.trim_end is not None:
                        original_file = d.get('filename')
                        if original_file:
                            # Check if this is an intermediate stream file (contains format ID markers)
                            # These will be merged later, so don't trim them yet
                            filename_lower = original_file.lower()
                            is_intermediate = any(marker in filename_lower for marker in [
                                '.fdash-', '.dash-', '.dash_sep-', '.f251-', '.f140-', '.m4a.', '.webm.'
                            ])
                            
                            if not is_intermediate:
                                trimmed_file = self._apply_trim_to_file(original_file)
                                if trimmed_file and trimmed_file != original_file:
                                    # Update filename to trimmed version
                                    d = dict(d)
                                    d['filename'] = trimmed_file
                                    # Update file size
                                    try:
                                        trimmed_path = Path(trimmed_file)
                                        if trimmed_path.exists():
                                            d['downloaded_bytes'] = trimmed_path.stat().st_size
                                            d['total_bytes'] = trimmed_path.stat().st_size
                                    except Exception as file_size_err:
                                        if config.DEBUG_MODE:
                                            print(f"{Fore.YELLOW}⚠️  Could not update file size: {file_size_err}")
                            elif config.DEBUG_MODE:
                                print(f"{Fore.CYAN}ℹ️  Skipping trim for intermediate stream: {Path(original_file).name}")
                except Exception as trim_err:
                    print(f"{Fore.YELLOW}⚠️  Error applying trim in progress hook: {trim_err}")
        except Exception as hook_err:
            print(f"{Fore.YELLOW}⚠️  Error in progress hook processing: {hook_err}")

        if self.progress_hook_callback:
            try:
                self.progress_hook_callback(d)
            except Exception as callback_err:
                print(f"{Fore.RED}❌ Error in progress callback: {callback_err}")

    def _validate_and_fix_file(self, filename: str) -> Optional[str]:
        """Check media metadata (duration, resolution). If invalid, try to remux with ffmpeg.

        Returns the path to a validated file (may be same as input) or None on failure.
        """
        try:
            path = Path(filename)
            if not path.exists():
                return None

            # Run ffprobe to get duration and stream info
            info = self._ffprobe(path)
            duration = info.get('duration', 0)
            width = info.get('width', 0)
            has_audio = info.get('has_audio')

            # Check if video has no audio (common issue with Dzen.ru)
            if has_audio is False and config.DEBUG_MODE:
                print(f"{Fore.YELLOW}⚠️  Warning: Downloaded file has no audio stream!")
                print(f"{Fore.YELLOW}   This is a known issue with some Dzen.ru videos")
                print(f"{Fore.YELLOW}   File: {path.name}")

            # If duration or width are missing/zero, attempt to remux to mp4
            if (not duration or duration <= 0) or (not width or width <= 0):
                # Choose target path with .mp4 extension in same dir
                target = path.with_suffix('.mp4')
                # If target exists, append suffix
                if target.exists():
                    target = path.with_name(path.stem + '_fixed.mp4')

                ok = self._remux_to_mp4(str(path), str(target))
                if ok and Path(target).exists():
                    try:
                        # Replace original file with fixed file if appropriate
                        # Keep original as backup with .orig suffix
                        backup = path.with_suffix(path.suffix + '.orig')
                        if not backup.exists():
                            path.replace(backup)
                        Path(target).replace(path)
                        return str(path)
                    except Exception:
                        return str(target)
            return str(path)
        except Exception:
            return None

    def _ffprobe(self, path: Path) -> Dict[str, Any]:
        """Run ffprobe (using detected ffmpeg path) and return parsed basic info including audio check."""
        try:
            ffmpeg_cmd = getattr(self.merger, 'ffmpeg_path', 'ffmpeg')
            ffprobe_cmd = ffmpeg_cmd.replace('ffmpeg', 'ffprobe')
            
            # Security: validate that ffprobe_cmd doesn't contain suspicious characters
            if any(char in ffprobe_cmd for char in [';', '&', '|', '$', '`']):
                logger.warning(f"Suspicious ffprobe command detected: {ffprobe_cmd}")
                return {}
            
            # Get video stream info
            cmd = [ffprobe_cmd, '-v', 'error', '-select_streams', 'v:0', 
                   '-show_entries', 'stream=width,height', 
                   '-show_entries', 'format=duration', 
                   '-of', 'default=noprint_wrappers=1:nokey=1', str(path)]
            result = subprocess.run(cmd, capture_output=True, text=True, 
                                  encoding='utf-8', timeout=30)  # Increased timeout
            out = result.stdout.strip().splitlines()
            info: Dict[str, Any] = {}
            if out:
                # Expect duration, width, height in some order; try to parse
                try:
                    # If three lines: duration, width, height
                    if len(out) >= 3:
                        info['duration'] = float(out[0]) if out[0] else 0
                        info['width'] = int(out[1]) if out[1] else 0
                        info['height'] = int(out[2]) if out[2] else 0
                    elif len(out) == 2:
                        info['duration'] = float(out[0]) if out[0] else 0
                        info['width'] = int(out[1]) if out[1] else 0
                        info['height'] = 0
                    else:
                        info['duration'] = float(out[0]) if out[0] else 0
                except Exception:
                    pass
            
            # Check if audio stream exists
            try:
                audio_cmd = [ffprobe_cmd, '-v', 'error', '-select_streams', 'a:0',
                           '-show_entries', 'stream=codec_name',
                           '-of', 'default=noprint_wrappers=1:nokey=1', str(path)]
                audio_result = subprocess.run(audio_cmd, capture_output=True, text=True,
                                             encoding='utf-8', timeout=10)
                info['has_audio'] = bool(audio_result.stdout.strip())
            except Exception:
                info['has_audio'] = None  # Unknown
            
            return info
        except Exception:
            return {}

    def _remux_to_mp4(self, src: str, dst: str) -> bool:
        """Use ffmpeg to remux/copy streams into an MP4 container with faststart."""
        try:
            ffmpeg_cmd = getattr(self.merger, 'ffmpeg_path', 'ffmpeg')
            cmd = [ffmpeg_cmd, '-y', '-i', src, '-c', 'copy', '-movflags', '+faststart', dst]
            result = subprocess.run(cmd, capture_output=True, text=True, encoding='utf-8', timeout=300)
            return result.returncode == 0
        except Exception:
            return False
    
    def _trim_video(self, input_path: str, output_path: str, start_time: float, end_time: float) -> bool:
        """Trim video using FFmpeg with precise cutting.
        
        Args:
            input_path: Path to input video file
            output_path: Path to output trimmed video file
            start_time: Start time in seconds
            end_time: End time in seconds
            
        Returns:
            True if trim was successful, False otherwise
        """
        try:
            if not self.merger.ffmpeg_available:
                print(f"{Fore.RED}❌ FFmpeg not available for video trimming")
                return False
            
            ffmpeg_cmd = self.merger.ffmpeg_path
            duration = end_time - start_time
            
            print(f"{Fore.CYAN}✂️  Trimming video: {start_time}s - {end_time}s (duration: {duration}s)")
            
            # Use -ss before -i for faster seeking (input seeking)
            # Use -t for duration instead of -to for more reliable results
            # Use -c copy for fast cutting without re-encoding when possible
            # For precise cutting, we may need to re-encode around keyframes
            cmd = [
                ffmpeg_cmd,
                '-y',  # Overwrite output
                '-ss', str(start_time),  # Seek to start time
                '-i', input_path,  # Input file
                '-t', str(duration),  # Duration to encode
                '-c', 'copy',  # Copy streams without re-encoding (fast)
                '-avoid_negative_ts', 'make_zero',  # Handle timestamp issues
                output_path
            ]
            
            result = subprocess.run(
                cmd,
                capture_output=True,
                text=True,
                encoding='utf-8',
                timeout=600  # 10 minute timeout for trim operation
            )
            
            if result.returncode == 0:
                print(f"{Fore.GREEN}✅ Video trimmed successfully")
                return True
            else:
                print(f"{Fore.YELLOW}⚠️  Fast trim failed, trying with re-encoding...")
                # Fallback: re-encode for precise cutting
                cmd_reencode = [
                    ffmpeg_cmd,
                    '-y',
                    '-ss', str(start_time),
                    '-i', input_path,
                    '-t', str(duration),
                    '-c:v', 'libx264',  # Re-encode video
                    '-c:a', 'aac',  # Re-encode audio
                    '-preset', 'fast',  # Fast encoding preset
                    '-movflags', '+faststart',  # Optimize for streaming
                    output_path
                ]
                
                result = subprocess.run(
                    cmd_reencode,
                    capture_output=True,
                    text=True,
                    encoding='utf-8',
                    timeout=1800  # 30 minute timeout for re-encoding
                )
                
                if result.returncode == 0:
                    print(f"{Fore.GREEN}✅ Video trimmed with re-encoding")
                    return True
                else:
                    print(f"{Fore.RED}❌ Trim failed: {result.stderr}")
                    return False
                    
        except subprocess.TimeoutExpired:
            print(f"{Fore.RED}❌ Trim operation timed out")
            return False
        except Exception as e:
            print(f"{Fore.RED}❌ Trim error: {e}")
            return False
    
    def _apply_trim_to_file(self, filepath: str) -> Optional[str]:
        """Apply trim to downloaded file if trim parameters are set.
        
        Args:
            filepath: Path to the downloaded file
            
        Returns:
            Path to trimmed file, or original filepath if trim not applied
        """
        try:
            # Check if trim is needed
            trim_start = getattr(self, 'trim_start', None)
            trim_end = getattr(self, 'trim_end', None)
            
            if trim_start is None and trim_end is None:
                return filepath  # No trim needed
            
            # Parse trim parameters
            start = float(trim_start) if trim_start is not None else 0
            end = float(trim_end) if trim_end is not None else 999999  # Large number if no end
            
            # Validate trim parameters
            if start < 0:
                start = 0
            if end <= start:
                print(f"{Fore.YELLOW}⚠️  Invalid trim parameters (end <= start), skipping trim")
                return filepath
            
            # Create trimmed filename
            original_path = Path(filepath)
            trimmed_filename = original_path.stem + '_trimmed' + original_path.suffix
            trimmed_path = original_path.parent / trimmed_filename
            
            # Perform trim
            print(f"{Fore.CYAN}✂️  Applying trim to downloaded file...")
            success = self._trim_video(str(original_path), str(trimmed_path), start, end)
            
            if success and trimmed_path.exists():
                # Remove original file with retry for Windows
                try:
                    # Wait briefly to ensure file handles are released
                    time.sleep(0.5)
                    original_path.unlink()
                    print(f"{Fore.GREEN}✅ Removed original file, keeping trimmed version")
                except Exception as e:
                    print(f"{Fore.YELLOW}⚠️  Could not remove original file: {e}")
                
                # Rename trimmed file to original name with retry for Windows
                for attempt in range(3):
                    try:
                        time.sleep(0.3)  # Brief delay to ensure file handles are released
                        final_path = trimmed_path.rename(original_path)
                        return str(final_path)
                    except PermissionError as e:
                        if attempt < 2:
                            print(f"{Fore.YELLOW}⚠️  File in use, retrying rename (attempt {attempt + 1}/3)...")
                            time.sleep(1.0)  # Wait longer on permission errors
                        else:
                            print(f"{Fore.YELLOW}⚠️  Could not rename trimmed file after 3 attempts: {e}")
                            print(f"{Fore.YELLOW}   Using trimmed filename instead")
                            return str(trimmed_path)
                    except Exception as e:
                        print(f"{Fore.YELLOW}⚠️  Could not rename trimmed file: {e}")
                        return str(trimmed_path)
            else:
                print(f"{Fore.YELLOW}⚠️  Trim failed, keeping original file")
                return filepath
                
        except Exception as e:
            print(f"{Fore.RED}❌ Error applying trim: {e}")
            return filepath
            
    def get_video_info(self, url: str) -> Optional[Dict[str, Any]]:
        """Get video information for web interface compatibility with improved error handling."""
        try:
            base_opts = {
                'quiet': True,
                'no_warnings': True,
                'extract_flat': False,
                'socket_timeout': 30,  # 30 second timeout
                'http_chunk_size': 1048576,  # 1MB chunks
            }
            
            # Apply robust options
            opts = self.error_handler.get_robust_options(base_opts)
            
            # Additional platform-specific optimizations
            import platform
            if platform.system() in ['Darwin', 'Linux']:
                # Mac/Linux specific optimizations
                opts.update({
                    'http_headers': {
                        **opts.get('http_headers', {}),
                        'Accept-Encoding': 'gzip, deflate',
                        'Connection': 'keep-alive',
                    },
                    'socket_timeout': 45,  # Longer timeout for Mac/Linux
                    'retries': 3,
                    'fragment_retries': 3,
                })
            
            # Apply SSL options
            opts = self._apply_ssl_options(opts)

            with yt_dlp.YoutubeDL(opts) as ydl:
                info = ydl.extract_info(url, download=False)
                return info
                
        except Exception as e:
            err_str = str(e)
            print(f"{Fore.RED}❌ Error getting video info: {e}")

            # If it's an SSL certificate verification issue, try with nocheckcertificate
            if self._is_ssl_error(err_str) and not self.insecure_ssl:
                print(f"{Fore.YELLOW}⚠️  SSL certificate verification failed. Retrying with 'nocheckcertificate'=True (insecure).")
                try:
                    ssl_opts = base_opts.copy()
                    ssl_opts['nocheckcertificate'] = True
                    # Merge robust options
                    try:
                        ssl_opts = {**self.error_handler.get_robust_options({}), **ssl_opts}
                    except Exception:
                        pass

                    with yt_dlp.YoutubeDL(ssl_opts) as ydl:
                        info = ydl.extract_info(url, download=False)
                        print(f"{Fore.GREEN}✅ Retrieved info using nocheckcertificate fallback")
                        return info
                except Exception as ssl_e:
                    err_str = str(ssl_e)
                    print(f"{Fore.RED}❌ SSL-fallback failed: {ssl_e}")
                    
                    # Check for JSON parse errors which indicate the site may be blocking
                    if 'JSON' in err_str or 'parse' in err_str.lower():
                        print(f"{Fore.YELLOW}⚠️  Site appears to be blocking requests or requires authentication")
                        print(f"{Fore.YELLOW}   Try: 1) Check if video is public, 2) Use cookies file, 3) Try different URL")
                        return None

            # Try fallback with minimal options
            try:
                fallback_opts = {
                    'quiet': True,
                    'no_warnings': True,
                    'extract_flat': False,
                    'socket_timeout': 20,
                    'retries': 1,
                }
                with yt_dlp.YoutubeDL(fallback_opts) as ydl:
                    info = ydl.extract_info(url, download=False)
                    return info
            except Exception as fallback_e:
                print(f"{Fore.RED}❌ Fallback also failed: {fallback_e}")
                return None
    
    def download_video(self, url: str, quality: str = "best", audio_only: bool = False, 
                      output_name: Optional[str] = None) -> bool:
        """Legacy method name for web interface compatibility."""
        return self.download(url, quality, "auto", output_name, audio_only)
        
    def download(self, url: str, quality: str = "best", mode: str = "auto", 
                output_name: Optional[str] = None, audio_only: bool = False) -> bool:
        """
        Universal download method with intelligent mode selection.
        
        Args:
            url: Video URL (YouTube, VK, Yandex, etc.)
            quality: Video quality (best, 4k, 1080p, 720p, 480p, 360p)
            mode: Download mode (auto, ultra, standard)
            output_name: Custom output filename
            audio_only: Download audio only
        """
        # Validate URL
        if not url or not isinstance(url, str):
            print(f"{Fore.RED}❌ Invalid URL provided")
            return False
        
        url = url.strip()
        if not url:
            print(f"{Fore.RED}❌ URL cannot be empty")
            return False
        
        # Basic URL validation
        if not url.startswith(('http://', 'https://')):
            print(f"{Fore.RED}❌ URL must start with http:// or https://")
            return False
        
        print(f"{Fore.MAGENTA}🎬 Unified Video Downloader")
        print(f"{Fore.CYAN}🎯 URL: {url}")
        print(f"{Fore.CYAN}📺 Quality: {quality}")
        
        if audio_only:
            return self._download_audio_only(url, output_name)
        
        # Determine best mode
        if mode == "auto":
            # Auto mode now prefers standard mode unless we have FFmpeg
            # MoviePy alone can't help with downloading separate streams from yt-dlp
            mode = "ultra" if self.merger.ffmpeg_available else "standard"
            
        print(f"{Fore.GREEN}⚡ Mode: {mode.upper()}")
        
        # Quick check for video availability before attempting ultra mode
        if mode == "ultra" and self.merger.available:
            # Check if video has actual video streams for ultra mode
            try:
                video_info = self._get_video_info(url)
                if video_info:
                    formats = video_info.get('formats', [])
                    has_video = any(f.get('vcodec') and f.get('vcodec') != 'none' for f in formats)
                    if not has_video:
                        print(f"{Fore.YELLOW}⚠️  No video streams detected, using standard mode")
                        mode = "standard"
            except Exception as e:
                print(f"{Fore.YELLOW}⚠️  Error checking video streams: {e}")
                pass  # If check fails, continue with original mode
                
        if mode == "ultra" and self.merger.available:
            return self._download_ultra_mode(url, quality, output_name)
        else:
            return self._download_standard_mode(url, quality, output_name)
    
    def _download_ultra_mode(self, url: str, quality: str, output_name: Optional[str]) -> bool:
        """Ultra mode with separate stream downloading and Python merging."""
        if config.DEBUG_MODE:
            print(f"{Fore.MAGENTA}🎬 ULTRA MODE - Pure Python Excellence")
        
        # Check if we can actually handle separate streams end-to-end
        # Note: yt-dlp needs FFmpeg to download separate streams, even if we can merge with MoviePy
        if not self.merger.ffmpeg_available:
            if config.DEBUG_MODE:
                if self.merger.available:
                    print(f"{Fore.YELLOW}⚠️  MoviePy available but FFmpeg needed for separate stream downloads")
                else:
                    print(f"{Fore.YELLOW}⚠️  No merging capability available (MoviePy or FFmpeg needed)")
                print(f"{Fore.YELLOW}🔄 Falling back to standard mode with enhanced quality...")
            return self._download_standard_mode(url, quality, output_name)
        
        try:
            if self._is_cancelled():
                print(f"{Fore.YELLOW}⚠️  Download cancelled (ultra mode)")
                return False
            with tempfile.TemporaryDirectory() as temp_dir:
                # Get video info
                video_info = self._get_video_info(url)
                if not video_info:
                    print(f"{Fore.RED}❌ Failed to get video info")
                    return False
                
                title = video_info.get('title', 'video')
                if config.DEBUG_MODE:
                    print(f"{Fore.GREEN}📺 {title}")
                
                # Smart format selection
                video_format, audio_format = self._select_formats(video_info, quality)
                
                if video_format is None or audio_format is None:
                    print(f"{Fore.YELLOW}🔄 Falling back to standard mode...")
                    return self._download_standard_mode(url, quality, output_name)
                
                if video_format and audio_format:
                    if config.DEBUG_MODE:
                        print(f"{Fore.CYAN}⬇️  Starting download...")
                    # Download separate streams
                    video_file, audio_file = self._download_separate_streams(
                        url, temp_dir, video_format, audio_format
                    )
                    
                    if video_file and audio_file:
                        # Always prefer FFmpeg for merging if available
                        output_path = self._get_output_path(title, output_name)
                        if config.DEBUG_MODE:
                            print(f"{Fore.YELLOW}🔄 Merging streams...")
                        if self.merger.ffmpeg_available:
                            if config.DEBUG_MODE:
                                print(f"{Fore.CYAN}Using FFmpeg for merging...")
                            success = self._merge_with_ytdlp(video_file, audio_file, str(output_path))
                        elif self.merger.available:
                            if config.DEBUG_MODE:
                                print(f"{Fore.CYAN}Using MoviePy for merging...")
                            success = self.merger.merge_streams(video_file, audio_file, str(output_path))
                        else:
                            print(f"{Fore.RED}❌ No merging capability available!")
                            success = False
                        if success:
                            print(f"{Fore.GREEN}✅ Download complete: {output_path.name}")
                            # Manually trigger completion for web interface
                            if hasattr(self, 'progress_hook_callback') and self.progress_hook_callback:
                                completion_data = {
                                    'status': 'finished',
                                    'filename': str(output_path),
                                    'downloaded_bytes': output_path.stat().st_size if output_path.exists() else 0,
                                    'total_bytes': output_path.stat().st_size if output_path.exists() else 0
                                }
                                self.progress_hook_callback(completion_data)
                            return True
                        else:
                            print(f"{Fore.YELLOW}⚠️  Merge failed, falling back...")
                    else:
                        print(f"{Fore.YELLOW}⚠️  Stream download failed, falling back...")
                else:
                    if config.DEBUG_MODE:
                        print(f"{Fore.YELLOW}⚠️  No suitable separate streams found, falling back...")
                
                # Fallback to standard mode
                if config.DEBUG_MODE:
                    print(f"{Fore.YELLOW}🔄 Falling back to standard mode...")
                return self._download_standard_mode(url, quality, output_name)
                
        except Exception as e:
            if config.DEBUG_MODE:
                print(f"{Fore.RED}❌ Ultra mode failed: {e}")
                print(f"{Fore.YELLOW}🔄 Falling back to standard mode...")
            return self._download_standard_mode(url, quality, output_name)
    
    def _download_standard_mode(self, url: str, quality: str, output_name: Optional[str]) -> bool:
        """Standard mode with combined streams."""
        print(f"{Fore.CYAN}📥 STANDARD MODE - Reliable Download")
        print(f"{Fore.CYAN}🎯 Target quality: {quality}")

        # Check if this is Dzen.ru
        is_dzen = 'dzen.ru' in url or 'zen.yandex' in url

        # Check if audio language is specified
        selected_audio_lang = getattr(self, 'audio_language', None)
        if selected_audio_lang and config.DEBUG_MODE:
            print(f"{Fore.CYAN}🌐 Requested audio language: {selected_audio_lang}")

        # Progressive quality fallback for best success rate
        quality_options = self._get_quality_fallbacks(quality, is_dzen=is_dzen)

        print(f"{Fore.CYAN}📋 Trying formats: {', '.join(quality_options[:3])}...")

        for attempt, fmt in enumerate(quality_options):
            if self._is_cancelled():
                print(f"{Fore.YELLOW}⚠️  Download cancelled (standard mode)")
                return False
            try:
                if config.DEBUG_MODE:
                    print(f"{Fore.YELLOW}🔍 Attempting format: {fmt}")
                
                # Modify format string to include audio language if specified
                format_str = fmt
                if selected_audio_lang:
                    # Add language filter to the format selector
                    # For combined formats: best[language=lang]
                    # For separate streams: bestvideo+bestaudio[language=lang]
                    if '+' in format_str:
                        # Separate streams format
                        parts = format_str.split('+')
                        if len(parts) == 2:
                            format_str = f"{parts[0]}+bestaudio[language={selected_audio_lang}]/{format_str}"
                    else:
                        # Try to prefer the selected language in combined format
                        format_str = f"{fmt}[language={selected_audio_lang}]/{fmt}"
                    if config.DEBUG_MODE:
                        print(f"{Fore.CYAN}🌐 Using audio language filter: {selected_audio_lang}")
                
                output_template = self._get_output_template(output_name)
                
                opts = {
                    'format': format_str,
                    'outtmpl': output_template,
                    'writeinfojson': False,
                    'writesubtitles': False,
                    'nooverwrites': False,  # Always re-download, don't skip existing files
                    'quiet': not config.DEBUG_MODE,  # Hide yt-dlp output when DEBUG_MODE is off
                    'no_warnings': not config.DEBUG_MODE,  # Hide warnings when DEBUG_MODE is off
                }
                
                # Only add metadata postprocessors if FFmpeg is available
                if self.merger.ffmpeg_available:
                    opts['embed_metadata'] = True
                    opts['embed_thumbnail'] = True
                    opts['addmetadata'] = True
                    opts['postprocessors'] = [
                        {
                            'key': 'FFmpegMetadata',
                            'add_metadata': True,
                        },
                        {
                            'key': 'EmbedThumbnail',
                            'already_have_thumbnail': False,
                        }
                    ]
                else:
                    # Skip metadata processing if FFmpeg not available
                    opts['postprocessors'] = []
                
                # Add progress hook if available
                if self.progress_hook_callback:
                    opts['progress_hooks'] = [self._progress_hook]
                
                # Apply error recovery on retries
                if attempt > 0:
                    print(f"{Fore.YELLOW}🔄 Retry {attempt + 1} with format: {fmt}")
                    opts = self.error_handler.get_robust_options(opts)
                    time.sleep(random.uniform(2, 5))  # Longer delay for better success
                
                opts = self._add_cookies_option(opts)
                if not self._ydl_download_with_ssl_fallback(opts, url):
                    raise Exception('Download failed')
                    
                if config.DEBUG_MODE:
                    print(f"{Fore.GREEN}✅ Download completed successfully with format: {fmt}")
                return True
                
            except Exception as e:
                error_msg = str(e)
                print(f"{Fore.RED}❌ Format {fmt} failed: {error_msg}")
                
                if "403" in error_msg or "Forbidden" in error_msg:
                    print(f"{Fore.YELLOW}🛡️ 403 Forbidden error detected on attempt {attempt + 1}")
                    
                    # Try 403-specific recovery
                    if attempt < 2:  # Allow 2 more attempts with 403 recovery
                        opts = self.error_handler.handle_403_error(url, opts, attempt)
                        print(f"{Fore.CYAN}🔄 Retrying with enhanced 403 recovery...")
                        continue
                    else:
                        print(f"{Fore.RED}❌ 403 error persists after recovery attempts")
                        
                elif "404" in error_msg:
                    print(f"{Fore.RED}❌ Video not available (404)")
                    return False
                elif "not available" in error_msg.lower():
                    print(f"{Fore.YELLOW}⚠️  Format not available, trying next...")
                    continue
                else:
                    print(f"{Fore.YELLOW}⚠️  Error: {error_msg}")
                    if attempt < len(quality_options) - 1:
                        time.sleep(random.uniform(1, 3))
                        continue
        
        print(f"{Fore.RED}❌ All download attempts failed")
        
        # Additional helpful message for format limitations
        if not self.merger.ffmpeg_available:
            print(f"\n{Fore.YELLOW}💡 Troubleshooting Tips:")
            print(f"{Fore.YELLOW}   • This video may only have high-quality content in separate streams")
            print(f"{Fore.YELLOW}   • Install FFmpeg to download the best available quality")
            print(f"{Fore.YELLOW}   • Try a different video that has combined formats available")
            print(f"{Fore.YELLOW}   • Use --list-formats to see all available formats")
        
        return False
    
    def _download_audio_only(self, url: str, output_name: Optional[str]) -> bool:
        """Download audio only."""
        print(f"{Fore.GREEN}🎵 Audio-only download")
        
        output_template = self._get_output_template(output_name, audio_only=True)
        
        opts = {
            'format': 'bestaudio/best',
            'outtmpl': output_template,
            'writeinfojson': False,
            'nooverwrites': False,  # Always re-download, don't skip existing files
            'quiet': not config.DEBUG_MODE,
            'no_warnings': not config.DEBUG_MODE,
        }
        
        # Only add FFmpeg postprocessors if FFmpeg is available
        if self.merger.ffmpeg_available:
            opts['embed_metadata'] = True
            opts['embed_thumbnail'] = True
            opts['addmetadata'] = True
            opts['postprocessors'] = [
                {
                    'key': 'FFmpegMetadata',
                    'add_metadata': True,
                },
                {
                    'key': 'EmbedThumbnail',
                    'already_have_thumbnail': False,
                },
                {
                    'key': 'FFmpegExtractAudio',
                    'preferredcodec': 'mp3',
                    'preferredquality': '192',
                }
            ]
        else:
            # Without FFmpeg, just download best audio format as-is
            if config.DEBUG_MODE:
                print(f"{Fore.YELLOW}⚠️  FFmpeg not available - downloading audio without conversion")
            opts['postprocessors'] = []
        
        # Add progress hook if available
        if self.progress_hook_callback:
            opts['progress_hooks'] = [self._progress_hook]
        
        opts = self._add_cookies_option(opts)
        try:
            if self._is_cancelled():
                print(f"{Fore.YELLOW}⚠️  Download cancelled (audio only)")
                return False
            if not self._ydl_download_with_ssl_fallback(opts, url):
                print(f"{Fore.RED}❌ Audio download failed")
                return False
            print(f"{Fore.GREEN}✅ Audio download completed")
            return True
        except Exception as e:
            print(f"{Fore.RED}❌ Audio download failed: {e}")
            return False
    
    def _get_video_info(self, url: str) -> Optional[Dict]:
        """Get video information with error recovery."""
        for attempt in range(3):
            try:
                opts = {'quiet': True, 'no_warnings': True, 'extract_flat': False}
                
                # For VK and other strict sites, use robust options from the start
                if 'vk.com' in url or 'vkontakte' in url:
                    opts = self.error_handler.get_robust_options(opts)
                elif attempt > 0:
                    opts = self.error_handler.get_robust_options(opts)
                    time.sleep(random.uniform(1, 3))
                
                opts = self._add_cookies_option(opts)
                opts = self._apply_ssl_options(opts)
                
                with yt_dlp.YoutubeDL(opts) as ydl:
                    return ydl.extract_info(url, download=False)
                    
            except Exception as e:
                err_str = str(e)
                
                # SSL fallback only if not already using insecure mode
                if self._is_ssl_error(err_str) and not self.insecure_ssl:
                    print(f"{Fore.YELLOW}⚠️  SSL certificate verification failed. Retrying with 'nocheckcertificate'=True.")
                    try:
                        ssl_opts = {**opts, 'nocheckcertificate': True}
                        with yt_dlp.YoutubeDL(ssl_opts) as ydl:
                            return ydl.extract_info(url, download=False)
                    except Exception as ssl_e:
                        ssl_err_str = str(ssl_e)
                        print(f"{Fore.RED}❌ SSL-fallback failed: {ssl_e}")
                        
                        # Check for JSON parse errors (VK blocking)
                        if 'JSON' in ssl_err_str or 'parse' in ssl_err_str.lower():
                            print(f"{Fore.YELLOW}⚠️  VK may be blocking requests or video requires authentication")
                            print(f"{Fore.YELLOW}   Solutions: 1) Check video privacy settings, 2) Use cookies.txt, 3) Try browser login")
                            return None

                if attempt == 2:
                    # Provide helpful message for common VK errors
                    if 'vk.com' in url and ('JSON' in err_str or 'parse' in err_str.lower()):
                        print(f"{Fore.RED}❌ Failed to get VK video info: {e}")
                        print(f"{Fore.YELLOW}💡 VK videos often require authentication or have strict privacy settings")
                        print(f"{Fore.YELLOW}   Try using a cookies file from your browser session")
                    else:
                        print(f"{Fore.RED}❌ Failed to get video info: {e}")
                    
        return None

    def _ydl_download_with_ssl_fallback(self, opts: Dict[str, Any], url: str) -> bool:
        """Run yt-dlp download with SSL-fallback retry (nocheckcertificate=True).

        Returns True on success, False on failure.
        """
        try:
            opts = self._inject_ffmpeg_location(opts)
            with yt_dlp.YoutubeDL(opts) as ydl:
                ydl.download([url])
            return True
        except Exception as e:
            err_str = str(e)
            
            # SSL fallback only if not already using insecure mode
            if self._is_ssl_error(err_str) and not self.insecure_ssl:
                print(f"{Fore.YELLOW}⚠️  SSL certificate verification failed during download. Retrying with 'nocheckcertificate'=True.")
                try:
                    ssl_opts = {**opts, 'nocheckcertificate': True}
                    ssl_opts = self._inject_ffmpeg_location(ssl_opts)
                    with yt_dlp.YoutubeDL(ssl_opts) as ydl:
                        ydl.download([url])
                    print(f"{Fore.GREEN}✅ Download succeeded using nocheckcertificate fallback")
                    return True
                except Exception as ssl_e:
                    print(f"{Fore.RED}❌ SSL-fallback download failed: {ssl_e}")
                    return False
            else:
                print(f"{Fore.RED}❌ Download failed: {e}")
                return False
    
    def _select_formats(self, video_info: Dict, quality: str) -> Tuple[Optional[str], Optional[str]]:
        """Smart format selection for separate streams with better validation."""
        formats = video_info.get('formats', [])
        
        # Special handling for Dzen.ru - use combined formats instead of separate streams
        url = video_info.get('webpage_url', '') or video_info.get('original_url', '')
        is_dzen = 'dzen.ru' in url or 'zen.yandex' in url
        if is_dzen and config.DEBUG_MODE:
            print(f"{Fore.CYAN}ℹ️  Dzen.ru detected - using combined formats for reliability")
        
        quality_heights = {
            'best': 2160, '4k': 2160, '1440p': 1440,
            '1080p': 1080, '720p': 720, '480p': 480, '360p': 360
        }
        target_height = quality_heights.get(quality.lower(), 1080)
        
        # Get selected audio language if available
        selected_audio_lang = getattr(self, 'audio_language', None)
        
        # Check if formats are muxed (both video and audio) or separate
        # Muxed formats should not be used in ULTRA mode
        has_muxed_only = all(
            (f.get('vcodec') and f.get('vcodec') != 'none' and 
             f.get('acodec') and f.get('acodec') != 'none')
            for f in formats if f.get('vcodec') and f.get('vcodec') != 'none'
        )
        
        if has_muxed_only:
            if config.DEBUG_MODE:
                print(f"{Fore.YELLOW}⚠️  All formats are muxed (video+audio combined)")
                print(f"{Fore.YELLOW}   Ultra mode requires separate streams - use standard mode instead")
            return None, None
        
        # Find video-only formats with better filtering and reliability scoring
        video_formats = []
        for f in formats:
            if (f.get('vcodec') and f.get('vcodec') != 'none' and 
                (f.get('acodec') == 'none' or not f.get('acodec')) and 
                f.get('height') and f.get('format_id') and 
                f.get('url')):  # Ensure format has actual URL
                
                # Check format reliability - prefer https over m3u8
                protocol = f.get('protocol', '')
                format_note = f.get('format_note', '')
                is_reliable = 'https' in protocol and 'm3u8' not in protocol and 'Untested' not in format_note
                
                f['_reliability_score'] = 1 if is_reliable else 0
                video_formats.append(f)
        
        # Find audio-only formats with better filtering and reliability scoring
        audio_formats = []
        for f in formats:
            if (f.get('acodec') and f.get('acodec') != 'none' and 
                (f.get('vcodec') == 'none' or not f.get('vcodec')) and 
                f.get('format_id') and f.get('url')):  # Ensure format has actual URL
                
                # Check format reliability
                protocol = f.get('protocol', '')
                format_note = f.get('format_note', '')
                is_reliable = 'https' in protocol and 'm3u8' not in protocol and 'Untested' not in format_note
                
                f['_reliability_score'] = 1 if is_reliable else 0
                
                # Filter by audio language if specified
                if selected_audio_lang:
                    fmt_lang = f.get('language') or f.get('lang')
                    if fmt_lang and fmt_lang == selected_audio_lang:
                        audio_formats.append(f)
                else:
                    audio_formats.append(f)
        
        # If no explicit separate streams available, signal to use standard mode
        if not video_formats or not audio_formats:
            if config.DEBUG_MODE:
                print(f"{Fore.YELLOW}⚠️  No explicit separate video/audio streams available (filtered)")
                print(f"{Fore.YELLOW}   Video formats matched: {len(video_formats)}")
                print(f"{Fore.YELLOW}   Audio formats matched: {len(audio_formats)}")
                if selected_audio_lang:
                    print(f"{Fore.YELLOW}   Requested audio language: {selected_audio_lang}")
                print(f"{Fore.YELLOW}   Returning None to trigger standard mode fallback")
            return None, None
        
        # Sort by preference: reliability first, then quality targeting
        video_formats.sort(key=lambda x: (
            -x.get('_reliability_score', 0),  # Prefer reliable formats first
            abs(x.get('height', 0) - target_height),
            -(x.get('height', 0)),
            -(x.get('tbr', 0) or x.get('vbr', 0) or 0)
        ))
        
        audio_formats.sort(key=lambda x: (
            -x.get('_reliability_score', 0),  # Prefer reliable formats first
            -(x.get('abr', 0) or x.get('tbr', 0) or 0)
        ))
        
        # Validate selected formats
        video_format = video_formats[0]
        audio_format = audio_formats[0]
        
        # Check if we're using unreliable formats
        video_reliable = video_format.get('_reliability_score', 0) == 1
        audio_reliable = audio_format.get('_reliability_score', 0) == 1
        
        if not video_reliable or not audio_reliable:
            if config.DEBUG_MODE:
                print(f"{Fore.YELLOW}⚠️  Using experimental formats - may be unreliable")
                if not video_reliable:
                    print(f"{Fore.YELLOW}   Video format {video_format['format_id']} is experimental")
                if not audio_reliable:
                    print(f"{Fore.YELLOW}   Audio format {audio_format['format_id']} is experimental")
                print(f"{Fore.YELLOW}   Consider using standard mode for better reliability")
        
        if config.DEBUG_MODE:
            print(f"{Fore.CYAN}📹 Selected video: {video_format['format_id']} ({video_format.get('height', 'unknown')}p)")
            print(f"{Fore.CYAN}🎵 Selected audio: {audio_format['format_id']} ({audio_format.get('abr', 'unknown')} kbps)")
        
        # Log selected audio language if specified
        if selected_audio_lang:
            audio_lang_name = audio_format.get('language_name') or audio_format.get('language') or selected_audio_lang
            if config.DEBUG_MODE:
                print(f"{Fore.CYAN}🌐 Audio language: {audio_lang_name} ({selected_audio_lang})")
        
        return video_format['format_id'], audio_format['format_id']
    
    def _download_separate_streams(self, url: str, temp_dir: str, 
                                  video_format: str, audio_format: str) -> Tuple[Optional[str], Optional[str]]:
        """Download video and audio streams in parallel."""
        video_file = None
        audio_file = None
        
        def download_video():
            nonlocal video_file
            try:
                if config.DEBUG_MODE:
                    print(f"{Fore.CYAN}⬇️  Downloading video stream: {video_format}")
                opts = self.error_handler.get_robust_options({
                    'format': video_format,
                    'outtmpl': f'{temp_dir}/video.%(ext)s',
                    'quiet': not config.DEBUG_MODE,
                    'no_warnings': not config.DEBUG_MODE,
                    'embed_metadata': True,
                    'embed_thumbnail': True,
                    'addmetadata': True,
                    'nooverwrites': False,  # Force re-download
                })
                
                # Add progress hook if available
                if self.progress_hook_callback:
                    opts['progress_hooks'] = [self._progress_hook]
                
                opts = self._add_cookies_option(opts)
                if not self._ydl_download_with_ssl_fallback(opts, url):
                    raise Exception('Video stream download failed')
                
                video_files = list(Path(temp_dir).glob('video.*'))
                video_file = str(video_files[0]) if video_files else None
                
                if video_file:
                    if config.DEBUG_MODE:
                        print(f"{Fore.GREEN}✅ Video stream downloaded successfully")
                else:
                    print(f"{Fore.RED}❌ Video file not found after download")
                
            except Exception as e:
                error_msg = str(e)
                if "403" in error_msg or "Forbidden" in error_msg:
                    print(f"{Fore.RED}🛡️ 403 Forbidden error in video stream download")
                    print(f"{Fore.YELLOW}💡 Try using standard mode: --mode standard")
                elif "not available" in error_msg.lower() or "requested format" in error_msg.lower():
                    print(f"{Fore.YELLOW}⚠️  Video format {video_format} not available")
                else:
                    print(f"{Fore.RED}❌ Video stream failed: {e}")
        
        def download_audio():
            nonlocal audio_file
            try:
                if config.DEBUG_MODE:
                    print(f"{Fore.CYAN}⬇️  Downloading audio stream: {audio_format}")
                opts = self.error_handler.get_robust_options({
                    'format': audio_format,
                    'outtmpl': f'{temp_dir}/audio.%(ext)s',
                    'quiet': not config.DEBUG_MODE,
                    'no_warnings': not config.DEBUG_MODE,
                    'embed_metadata': True,
                    'embed_thumbnail': True,
                    'addmetadata': True,
                    'nooverwrites': False,  # Force re-download
                })
                
                # Add progress hook if available
                if self.progress_hook_callback:
                    opts['progress_hooks'] = [self._progress_hook]
                
                opts = self._add_cookies_option(opts)
                if not self._ydl_download_with_ssl_fallback(opts, url):
                    raise Exception('Audio stream download failed')
                
                audio_files = list(Path(temp_dir).glob('audio.*'))
                audio_file = str(audio_files[0]) if audio_files else None
                
                if audio_file:
                    if config.DEBUG_MODE:
                        print(f"{Fore.GREEN}✅ Audio stream downloaded successfully")
                else:
                    print(f"{Fore.RED}❌ Audio file not found after download")
                
            except Exception as e:
                error_msg = str(e)
                if "403" in error_msg or "Forbidden" in error_msg:
                    print(f"{Fore.RED}🛡️ 403 Forbidden error in audio stream download")
                    print(f"{Fore.YELLOW}💡 Try using standard mode: --mode standard")
                elif "not available" in error_msg.lower() or "requested format" in error_msg.lower():
                    print(f"{Fore.YELLOW}⚠️  Audio format {audio_format} not available")
                else:
                    print(f"{Fore.RED}❌ Audio stream failed: {e}")
        
        # Parallel execution
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as executor:
            video_future = executor.submit(download_video)
            audio_future = executor.submit(download_audio)
            concurrent.futures.wait([video_future, audio_future])
        
        return video_file, audio_file
    
    def _merge_with_ytdlp(self, video_path: str, audio_path: str, output_path: str) -> bool:
        """Fallback merge using FFmpeg directly."""
        try:
            
            # Use the detected FFmpeg path
            ffmpeg_cmd = getattr(self.merger, 'ffmpeg_path', 'ffmpeg')
            
            cmd = [
                ffmpeg_cmd, '-i', video_path, '-i', audio_path, 
                '-c:v', 'copy', '-c:a', 'copy', 
                '-f', 'mp4', output_path, '-y'
            ]
            
            result = subprocess.run(cmd, capture_output=True, text=True, encoding='utf-8', timeout=300)
            
            if result.returncode == 0:
                print(f"{Fore.GREEN}✅ FFmpeg merge successful")
                return True
            else:
                print(f"{Fore.RED}❌ FFmpeg merge failed: {result.stderr}")
                return False
                
        except Exception as e:
            print(f"{Fore.RED}❌ FFmpeg merge error: {e}")
            return False
    
    def _get_quality_fallbacks(self, quality: str, is_dzen: bool = False) -> List[str]:
        """Get progressive quality fallback options with better high-quality selection."""
        
        # Special format selection for Dzen.ru - use combined formats with audio
        if is_dzen:
            # Dzen.ru needs special format strings that ensure audio is included
            # Use height limits to respect quality selection
            dzen_fallbacks = {
                'best': [
                    'best[ext=mp4]',
                    'bestvideo[ext=mp4]+bestaudio[ext=m4a]/best',
                    'best[height<=1080]',
                    'best[height<=720]',
                    'best'
                ],
                '4k': [
                    'best[height<=2160][ext=mp4]',
                    'best[height<=1440][ext=mp4]',
                    'bestvideo[height<=2160][ext=mp4]+bestaudio[ext=m4a]/best[height<=2160]',
                    'bestvideo[height<=1440][ext=mp4]+bestaudio[ext=m4a]/best[height<=1440]',
                    'best[ext=mp4]',
                    'best'
                ],
                '1440p': [
                    'best[height<=1440][ext=mp4]',
                    'best[height<=1080][ext=mp4]',
                    'bestvideo[height<=1440][ext=mp4]+bestaudio[ext=m4a]/best[height<=1440]',
                    'bestvideo[height<=1080][ext=mp4]+bestaudio[ext=m4a]/best[height<=1080]',
                    'best[ext=mp4]',
                    'best'
                ],
                '1080p': [
                    'best[height<=1080][ext=mp4]',
                    'best[height<=720][ext=mp4]',
                    'bestvideo[height<=1080][ext=mp4]+bestaudio[ext=m4a]/best[height<=1080]',
                    'bestvideo[height<=720][ext=mp4]+bestaudio[ext=m4a]/best[height<=720]',
                    'best[ext=mp4]',
                    'best'
                ],
                '720p': [
                    'best[height<=720][ext=mp4]',
                    'best[height<=480][ext=mp4]',
                    'bestvideo[height<=720][ext=mp4]+bestaudio[ext=m4a]/best[height<=720]',
                    'bestvideo[height<=480][ext=mp4]+bestaudio[ext=m4a]/best[height<=480]',
                    'best[ext=mp4]',
                    'best'
                ],
                '480p': [
                    'best[height<=480][ext=mp4]',
                    'best[height<=360][ext=mp4]',
                    'bestvideo[height<=480][ext=mp4]+bestaudio[ext=m4a]/best[height<=480]',
                    'bestvideo[height<=360][ext=mp4]+bestaudio[ext=m4a]/best[height<=360]',
                    'best[ext=mp4]',
                    'best'
                ],
                '360p': [
                    'best[height<=360][ext=mp4]',
                    'best[height<=240][ext=mp4]',
                    'bestvideo[height<=360][ext=mp4]+bestaudio[ext=m4a]/best[height<=360]',
                    'bestvideo[height<=240][ext=mp4]+bestaudio[ext=m4a]/best[height<=240]',
                    'best[ext=mp4]',
                    'best'
                ],
            }
            return dzen_fallbacks.get(quality.lower(), dzen_fallbacks['best'])
        
        # Check if we can handle separate streams
        # For standard mode, we need FFmpeg to download separate streams via yt-dlp
        # MoviePy can only merge after download, not facilitate the download itself
        can_merge = self.merger.ffmpeg_available
        
        if can_merge:
            # Full capability with merging - try separate streams first for best quality
            fallbacks = {
                'best': [
                    'bestvideo[height>=1080]+bestaudio/best[height>=1080]',
                    'bestvideo[height>=720]+bestaudio/best[height>=720]',
                    'bestvideo+bestaudio/best[height>=480]',
                    'best[height>=1080]', 'best[height>=720]', 'best[height>=480]', 
                    'best[height<=2160]', 'best[height<=1080]', 'best[height<=720]',
                    'bestvideo[ext=mp4]+bestaudio[ext=m4a]/best[ext=mp4]',
                    'bestvideo+bestaudio/best', 'best'
                ],
                '4k': [
                    'bestvideo[height>=2160]+bestaudio/best[height>=2160]',
                    'bestvideo[height>=1440]+bestaudio/best[height>=1440]',
                    'bestvideo[height>=1080]+bestaudio/best[height>=1080]',
                    'best[height>=2160]', 'best[height>=1440]', 'best[height>=1080]',
                    'best[height<=2160]', 'best[height<=1440]', 'best[height<=1080]', 
                    'bestvideo[height>=2160]+bestaudio/best[height<=2160]',
                    'bestvideo+bestaudio/best', 'best'
                ],
                '1440p': [
                    'bestvideo[height<=1440][height>=1080]+bestaudio/best[height<=1440][height>=1080]',  # Prefer 1440p, accept 1080p
                    'bestvideo[height<=1440]+bestaudio/best[height<=1440]',  # Max 1440p
                    'best[height<=1440][height>=1080]',  # Combined format, prefer 1440p/1080p
                    'best[height<=1440]', 'best[height>=1080]',
                    'bestvideo[height>=1080]+bestaudio/best[height>=1080]',
                    'bestvideo+bestaudio/best', 'best'
                ],
                '1080p': [
                    'bestvideo[height<=1080][height>=720]+bestaudio/best[height<=1080][height>=720]',  # Prefer 1080p, accept 720p
                    'bestvideo[height<=1080]+bestaudio/best[height<=1080]',  # Max 1080p
                    'best[height<=1080][height>=720]',  # Combined format, prefer 1080p/720p
                    'best[height<=1080]', 'best[height>=720]',
                    'bestvideo[height>=720]+bestaudio/best[height>=720]',
                    'bestvideo+bestaudio/best', 'best'
                ],
                '720p': [
                    'bestvideo[height<=720][height>=480]+bestaudio/best[height<=720][height>=480]',  # Prefer 720p, accept 480p
                    'bestvideo[height<=720]+bestaudio/best[height<=720]',  # Max 720p
                    'best[height<=720][height>=480]',  # Combined format, prefer 720p/480p
                    'best[height<=720]', 'best[height>=480]',
                    'bestvideo[height>=480]+bestaudio/best[height>=480]',
                    'bestvideo+bestaudio/best', 'best'
                ],
                '480p': [
                    'bestvideo[height<=480][height>=360]+bestaudio/best[height<=480][height>=360]',  # Prefer 480p, accept 360p
                    'bestvideo[height<=480]+bestaudio/best[height<=480]',  # Max 480p
                    'best[height<=480][height>=360]',  # Combined format
                    'best[height<=480]', 'best[height>=360]',
                    'bestvideo[height>=360]+bestaudio/best[height>=360]',
                    'bestvideo+bestaudio/best', 'best'
                ],
                '360p': [
                    'bestvideo[height<=360][height>=240]+bestaudio/best[height<=360][height>=240]',  # Prefer 360p, accept 240p
                    'bestvideo[height<=360]+bestaudio/best[height<=360]',  # Max 360p
                    'best[height<=360][height>=240]',  # Combined format
                    'best[height<=360]', 'best[height>=240]',
                    'bestvideo[height>=240]+bestaudio/best[height>=240]',
                    'bestvideo+bestaudio/best', 'best'
                ],
                '240p': [
                    'bestvideo[height<=240][height>=144]+bestaudio/best[height<=240][height>=144]',  # Prefer 240p, accept 144p
                    'bestvideo[height<=240]+bestaudio/best[height<=240]',  # Max 240p
                    'best[height<=240][height>=144]',  # Combined format
                    'best[height<=240]', 'best[height>=144]',
                    'bestvideo[height>=144]+bestaudio/best[height>=144]',
                    'bestvideo+bestaudio/best', 'best'
                ],
                '144p': [
                    'bestvideo[height<=144]+bestaudio/best[height<=144]',  # Max 144p
                    'best[height<=144]',  # Combined format
                    'bestvideo[height>=144]+bestaudio/best[height>=144]',
                    'bestvideo+bestaudio/best', 'best'
                ]
            }
        else:
            # No merging capability - prioritize single-file (muxed) formats only
            # Note: Most YouTube videos have combined formats available up to 360p-480p
            if config.DEBUG_MODE:
                print(f"{Fore.YELLOW}⚠️  No merging capability detected - using combined formats only")
                print(f"{Fore.YELLOW}   Combined formats typically available: 144p, 240p, 360p, 480p")
                print(f"{Fore.YELLOW}   Install FFmpeg for high-quality separate stream downloads")
            fallbacks = {
                '4k': [
                    'best[height>=2160][vcodec!*=none][acodec!*=none]',
                    'best[height>=1440][vcodec!*=none][acodec!*=none]',
                    'best[height>=1080][vcodec!*=none][acodec!*=none]',
                    'best[height>=720][vcodec!*=none][acodec!*=none]',
                    'best[vcodec!*=none][acodec!*=none]',
                    'best[ext=mp4]', 'best'
                ],
                '1440p': [
                    'best[height>=1440][vcodec!*=none][acodec!*=none]',
                    'best[height>=1080][vcodec!*=none][acodec!*=none]',
                    'best[height>=720][vcodec!*=none][acodec!*=none]',
                    'best[height>=480][vcodec!*=none][acodec!*=none]',
                    'best[vcodec!*=none][acodec!*=none]',
                    'best[ext=mp4]', 'best'
                ],
                '1080p': [
                    'best[height>=1080][vcodec!*=none][acodec!*=none]',
                    'best[height>=720][vcodec!*=none][acodec!*=none]',
                    'best[height>=480][vcodec!*=none][acodec!*=none]',
                    'best[height>=360][vcodec!*=none][acodec!*=none]',
                    'best[vcodec!*=none][acodec!*=none]',
                    'best[ext=mp4]', 'best'
                ],
                '720p': [
                    'best[height>=720][vcodec!*=none][acodec!*=none]',
                    'best[height>=480][vcodec!*=none][acodec!*=none]',
                    'best[height>=360][vcodec!*=none][acodec!*=none]',
                    'best[height>=240][vcodec!*=none][acodec!*=none]',
                    'best[vcodec!*=none][acodec!*=none]',
                    'best[ext=mp4]', 'best'
                ],
                '480p': [
                    'best[height=480][vcodec!*=none][acodec!*=none]',
                    'best[height>=480][height<720][vcodec!*=none][acodec!*=none]',
                    'best[height<=480][height>=360][vcodec!*=none][acodec!*=none]',
                    'best[height<=480][vcodec!*=none][acodec!*=none]',
                    'best[ext=mp4]', 'best'
                ],
                '360p': [
                    'best[height=360][vcodec!*=none][acodec!*=none]',
                    'best[height>=360][height<480][vcodec!*=none][acodec!*=none]',
                    'best[height<=360][height>=240][vcodec!*=none][acodec!*=none]',
                    'best[height<=360][vcodec!*=none][acodec!*=none]',
                    'best[ext=mp4]', 'best'
                ],
                '240p': [
                    'best[height=240][vcodec!*=none][acodec!*=none]',
                    'best[height>=240][height<360][vcodec!*=none][acodec!*=none]',
                    'best[height<=240][height>=144][vcodec!*=none][acodec!*=none]',
                    'best[height<=240][vcodec!*=none][acodec!*=none]',
                    'best[ext=mp4]', 'best', 'worst[height=240][vcodec!*=none][acodec!*=none]', 'worst[ext=mp4]', 'worst'
                ],
                '144p': [
                    'best[height=144][vcodec!*=none][acodec!*=none]',
                    'best[height>=144][height<240][vcodec!*=none][acodec!*=none]',
                    'best[height<=144][vcodec!*=none][acodec!*=none]',
                    'best[ext=mp4]', 'best', 'worst[height=144][vcodec!*=none][acodec!*=none]', 'worst[ext=mp4]', 'worst'
                ]
            }
        
        return fallbacks.get(quality.lower(), [
            'best[height>=720][vcodec!*=none][acodec!*=none]' if not can_merge else 'bestvideo[height>=720]+bestaudio/best[height>=720]', 
            'best[height>=480][vcodec!*=none][acodec!*=none]' if not can_merge else 'bestvideo[height>=480]+bestaudio/best[height>=480]',
            'best[height>=360][vcodec!*=none][acodec!*=none]' if not can_merge else 'bestvideo[height>=360]+bestaudio/best[height>=360]',
            'best[vcodec!*=none][acodec!*=none]' if not can_merge else 'bestvideo+bestaudio/best', 
            'best'
        ])
    
    def _get_output_template(self, output_name: Optional[str], audio_only: bool = False) -> str:
        """Generate output template."""
        ext = "mp3" if audio_only else "%(ext)s"
        if output_name:
            safe_name = re.sub(r'[<>:"/\\|?*]', '_', output_name)
            template = str(self.download_path / f"{safe_name}.{ext}")
            return template
        template = str(self.download_path / f"%(title)s.{ext}")
        return template
    
    def _get_output_path(self, title: str, output_name: Optional[str]) -> Path:
        """Generate safe output path."""
        if output_name:
            safe_name = re.sub(r'[<>:"/\\|?*]', '_', output_name)
        else:
            safe_name = re.sub(r'[<>:"/\\|?*]', '_', title)
        return self.download_path / f"{safe_name}.mp4"
    
    def get_formats(self, url: str) -> Optional[List[Dict]]:
        """Get available formats for a video."""
        try:
            video_info = self.get_video_info(url)
            if video_info:
                return video_info.get('formats', [])
        except Exception as e:
            print(f"{Fore.RED}❌ Error getting formats: {e}")
        return None
    
    def print_capabilities(self) -> None:
        """Print downloader capabilities."""
        print(f"\n{Fore.MAGENTA}🚀 Multi-Platform Downloader Capabilities")
        print(f"{Fore.CYAN}{'='*50}")
        print(f"{Fore.GREEN}✅ Advanced error recovery (403, 429, geo-blocking)")
        print(f"{Fore.GREEN}✅ Intelligent quality fallback")
        print(f"{Fore.GREEN}✅ Pure Python video merging (MoviePy): {'ENABLED' if self.merger.available else 'DISABLED'}")
        print(f"{Fore.GREEN}✅ FFmpeg external merging: {'ENABLED' if self.merger.ffmpeg_available else 'DISABLED'}")
        
        merge_status = "FULL" if self.merger.ffmpeg_available else ("PARTIAL" if self.merger.available else "LIMITED")
        merge_color = Fore.GREEN if merge_status == "FULL" else (Fore.YELLOW if merge_status == "PARTIAL" else Fore.RED)
        print(f"{merge_color}✅ Merging capability: {merge_status}")
        
        if merge_status == "PARTIAL":
            print(f"{Fore.YELLOW}⚠️  MoviePy available for merging, but FFmpeg needed for downloading separate streams")
            print(f"{Fore.YELLOW}   Current limitation: Combined formats only (usually up to 360p-480p)")
            print(f"{Fore.YELLOW}   For best quality: Install FFmpeg to enable separate stream downloads")
        elif merge_status == "LIMITED":
            print(f"{Fore.RED}⚠️  Single-file formats only (install MoviePy or FFmpeg for best quality)")
        
        print(f"{Fore.GREEN}✅ Multiple download modes (auto, ultra, standard)")
        print(f"{Fore.GREEN}✅ Audio-only downloads")
        print(f"{Fore.GREEN}✅ Custom output naming")
        print(f"{Fore.GREEN}✅ Format listing and inspection")
        print(f"{Fore.GREEN}✅ Web interface compatibility")
        print(f"{Fore.CYAN}{'='*50}")
        
    def _add_cookies_option(self, opts):
        cookies_path = os.path.join(os.path.dirname(__file__), 'cookies.txt')
        if os.path.exists(cookies_path):
            opts['cookiefile'] = cookies_path
        return opts
    
    def debug_available_formats(self, url: str) -> Dict[str, Any]:
        """Get all available formats for a video (for troubleshooting)."""
        try:
            opts = {
                'quiet': True,
                'no_warnings': True,
                'extractaudio': False,
                'audioformat': 'mp3',
                'outtmpl': '%(title)s.%(ext)s',
                'restrictfilenames': True,
            }
            opts = self._apply_ssl_options(opts)
            
            with yt_dlp.YoutubeDL(opts) as ydl:
                try:
                    info = ydl.extract_info(url, download=False)
                except Exception as e:
                    err_str = str(e)
                    # SSL fallback only if not already using insecure mode
                    if self._is_ssl_error(err_str) and not self.insecure_ssl:
                        print(f"{Fore.YELLOW}⚠️  SSL error while debugging formats. Retrying with 'nocheckcertificate'=True.")
                        ssl_opts = {**opts, 'nocheckcertificate': True}
                        with yt_dlp.YoutubeDL(ssl_opts) as ydl2:
                            info = ydl2.extract_info(url, download=False)
                    else:
                        raise
                
                formats = info.get('formats', [])
                
                print(f"{Fore.CYAN}🔍 DEBUG: Available formats for video:")
                print(f"{Fore.CYAN}   Title: {info.get('title', 'Unknown')}")
                print(f"{Fore.CYAN}   Total formats: {len(formats)}")
                
                # Group formats by height
                height_groups = {}
                for fmt in formats:
                    height = fmt.get('height')
                    if height:
                        if height not in height_groups:
                            height_groups[height] = []
                        height_groups[height].append({
                            'format_id': fmt.get('format_id'),
                            'ext': fmt.get('ext'),
                            'vcodec': fmt.get('vcodec'),
                            'acodec': fmt.get('acodec'),
                            'protocol': fmt.get('protocol'),
                            'tbr': fmt.get('tbr'),
                            'vbr': fmt.get('vbr'),
                            'abr': fmt.get('abr'),
                        })
                
                for height in sorted(height_groups.keys(), reverse=True):
                    print(f"{Fore.YELLOW}   {height}p: {len(height_groups[height])} formats")
                    for fmt in height_groups[height][:3]:  # Show first 3 formats
                        print(f"{Fore.WHITE}     - {fmt['format_id']}: {fmt['ext']}, v:{fmt['vcodec']}, a:{fmt['acodec']}, {fmt['protocol']}")
                
                return {'formats': formats, 'height_groups': height_groups}
                
        except Exception as e:
            print(f"{Fore.RED}❌ Error debugging formats: {str(e)}")
            return {}

def main():
    """Command line interface."""
    parser = argparse.ArgumentParser(description='Ultimate Multi-Platform Video Downloader')
    parser.add_argument('url', nargs='?', help='Video URL (YouTube, VK, Yandex, etc.)')
    parser.add_argument('-q', '--quality', default='best', 
                       choices=['best', '4k', '1440p', '1080p', '720p', '480p', '360p'],
                       help='Video quality (default: best)')
    parser.add_argument('-m', '--mode', default='auto', 
                       choices=['auto', 'ultra', 'standard'],
                       help='Download mode (default: auto)')
    parser.add_argument('-o', '--output', help='Output filename (without extension)')
    parser.add_argument('-d', '--download-path', default='./downloads', 
                       help='Download directory path')
    parser.add_argument('--audio-only', action='store_true', help='Download audio only')
    parser.add_argument('--list-formats', action='store_true', 
                       help='List available formats without downloading')
    parser.add_argument('--capabilities', action='store_true', 
                       help='Show downloader capabilities')
    
    args = parser.parse_args()
    
    downloader = YouTubeDownloader(args.download_path)
    
    if args.capabilities:
        downloader.print_capabilities()
        return
    
    if not args.url:
        parser.error("URL is required unless using --capabilities")
    
    if args.list_formats:
        formats = downloader.get_formats(args.url)
        if formats:
            print(f"\n{Fore.CYAN}Available formats for: {args.url}")
            for f in formats[:10]:  # Show first 10
                quality = f.get('format_note', 'unknown')
                ext = f.get('ext', 'unknown')
                size = f.get('filesize', 'unknown')
                print(f"{Fore.WHITE}  {f['format_id']}: {quality} ({ext}) - {size}")
        return
    
    print(f"\n{Fore.MAGENTA}🎬 Starting download...")
    success = downloader.download(
        args.url, 
        args.quality, 
        args.mode,
        args.output, 
        args.audio_only
    )
    
    if success:
        print(f"\n{Fore.GREEN}🎉 Download completed successfully!")
    else:
        print(f"\n{Fore.RED}❌ Download failed")
        sys.exit(1)


if __name__ == "__main__":
    main()
