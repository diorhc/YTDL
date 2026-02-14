import { useState, useRef, useEffect, useCallback } from "react";
import {
  Play,
  Pause,
  Volume2,
  VolumeX,
  Maximize,
  Minimize,
  Settings,
  Loader2,
  X,
  ExternalLink,
  SkipBack,
  SkipForward,
  PictureInPicture2,
  Download,
  AlertCircle,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { commands } from "@/lib/tauri";
import type { StreamInfo, StreamQuality } from "@/lib/tauri";

interface VideoPlayerProps {
  url: string;
  title: string;
  isShort?: boolean;
  onClose: () => void;
  onDownload?: () => void;
}

export function VideoPlayer({
  url,
  title,
  isShort = false,
  onClose,
  onDownload,
}: VideoPlayerProps) {
  const [streamInfo, setStreamInfo] = useState<StreamInfo | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Player state
  const videoRef = useRef<HTMLVideoElement>(null);
  const audioRef = useRef<HTMLAudioElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const progressRef = useRef<HTMLDivElement>(null);

  const [playing, setPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [buffered, setBuffered] = useState(0);
  const [volume, setVolume] = useState(1);
  const [muted, setMuted] = useState(false);
  const [isFullscreen, setIsFullscreen] = useState(false);
  const [showControls, setShowControls] = useState(true);
  const [showQualityMenu, setShowQualityMenu] = useState(false);
  const [selectedQuality, setSelectedQuality] = useState<StreamQuality | null>(
    null,
  );
  const [isSeparateStreams, setIsSeparateStreams] = useState(false);
  const hideControlsTimeout = useRef<ReturnType<typeof setTimeout>>(undefined);

  // Fetch stream URL
  useEffect(() => {
    let cancelled = false;
    const fetchStream = async () => {
      try {
        setLoading(true);
        setError(null);
        console.log("[VideoPlayer] Fetching stream for URL:", url);
        const info = await commands.getStreamUrl(url);
        console.log("[VideoPlayer] Stream info received:", info);
        if (cancelled) return;
        setStreamInfo(info);

        // Determine if we need separate audio/video
        const hasSeparate = !!(
          info.videoUrl &&
          info.audioUrl &&
          info.videoUrl !== info.combinedUrl
        );
        setIsSeparateStreams(hasSeparate);

        // Set default quality
        if (info.qualities && info.qualities.length > 0) {
          // Pick 720p or the closest
          const preferred =
            info.qualities.find((q) => q.height <= 1080) || info.qualities[0];
          setSelectedQuality(preferred);
        }

        setDuration(info.duration || 0);
      } catch (err) {
        console.error("[VideoPlayer] Error fetching stream:", err);
        if (cancelled) return;
        setError(String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    fetchStream();
    return () => {
      cancelled = true;
    };
  }, [url]);

  // Sync audio with video for separate streams
  const syncAudio = useCallback(() => {
    if (!isSeparateStreams || !videoRef.current || !audioRef.current) return;
    const video = videoRef.current;
    const audio = audioRef.current;

    if (Math.abs(video.currentTime - audio.currentTime) > 0.3) {
      audio.currentTime = video.currentTime;
    }
  }, [isSeparateStreams]);

  // Play/Pause
  const togglePlay = useCallback(() => {
    const video = videoRef.current;
    if (!video) return;

    if (video.paused) {
      const playPromise = video.play();
      if (playPromise !== undefined) {
        playPromise
          .then(() => {
            if (isSeparateStreams && audioRef.current) {
              audioRef.current.currentTime = video.currentTime;
              audioRef.current.play().catch((err) => {
                console.error("[VideoPlayer] Audio play failed:", err);
              });
            }
            setPlaying(true);
          })
          .catch((err) => {
            console.error("[VideoPlayer] Video play failed:", err);
            setError(`Failed to play video: ${err.message || String(err)}`);
          });
      }
    } else {
      video.pause();
      if (isSeparateStreams && audioRef.current) {
        audioRef.current.pause();
      }
      setPlaying(false);
    }
  }, [isSeparateStreams]);

  // Seek
  const handleSeek = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const percent = (e.clientX - rect.left) / rect.width;
      const newTime = percent * duration;
      if (videoRef.current) {
        videoRef.current.currentTime = newTime;
      }
      if (isSeparateStreams && audioRef.current) {
        audioRef.current.currentTime = newTime;
      }
      setCurrentTime(newTime);
    },
    [duration, isSeparateStreams],
  );

  // Volume
  const handleVolumeChange = useCallback(
    (e: React.MouseEvent<HTMLDivElement>) => {
      const rect = e.currentTarget.getBoundingClientRect();
      const newVol = Math.max(
        0,
        Math.min(1, (e.clientX - rect.left) / rect.width),
      );
      setVolume(newVol);
      setMuted(newVol === 0);
      if (isSeparateStreams && audioRef.current) {
        audioRef.current.volume = newVol;
        audioRef.current.muted = newVol === 0;
      } else if (videoRef.current) {
        videoRef.current.volume = newVol;
        videoRef.current.muted = newVol === 0;
      }
    },
    [isSeparateStreams],
  );

  const toggleMute = useCallback(() => {
    const newMuted = !muted;
    setMuted(newMuted);
    if (isSeparateStreams && audioRef.current) {
      audioRef.current.muted = newMuted;
    } else if (videoRef.current) {
      videoRef.current.muted = newMuted;
    }
  }, [muted, isSeparateStreams]);

  // Fullscreen
  const toggleFullscreen = useCallback(() => {
    if (!containerRef.current) return;
    if (document.fullscreenElement) {
      document.exitFullscreen();
      setIsFullscreen(false);
    } else {
      containerRef.current.requestFullscreen();
      setIsFullscreen(true);
    }
  }, []);

  // PiP
  const togglePiP = useCallback(async () => {
    if (!videoRef.current) return;
    try {
      if (document.pictureInPictureElement) {
        await document.exitPictureInPicture();
      } else {
        await videoRef.current.requestPictureInPicture();
      }
    } catch {
      // PiP not supported
    }
  }, []);

  // Quality change
  const changeQuality = useCallback(
    (quality: StreamQuality) => {
      setSelectedQuality(quality);
      setShowQualityMenu(false);

      if (videoRef.current) {
        const wasPlaying = !videoRef.current.paused;
        const time = videoRef.current.currentTime;
        videoRef.current.src = quality.url;
        videoRef.current.currentTime = time;
        if (wasPlaying) {
          videoRef.current.play();
          if (isSeparateStreams && audioRef.current) {
            audioRef.current.currentTime = time;
            audioRef.current.play();
          }
        }
      }
    },
    [isSeparateStreams],
  );

  // Skip forward/backward
  const skip = useCallback(
    (seconds: number) => {
      if (!videoRef.current) return;
      videoRef.current.currentTime += seconds;
      if (isSeparateStreams && audioRef.current) {
        audioRef.current.currentTime = videoRef.current.currentTime;
      }
    },
    [isSeparateStreams],
  );

  // Time update
  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    const onTimeUpdate = () => {
      setCurrentTime(video.currentTime);
      syncAudio();
    };
    const onDurationChange = () => {
      if (video.duration && !isNaN(video.duration)) {
        setDuration(video.duration);
      }
    };
    const onProgress = () => {
      if (video.buffered.length > 0) {
        setBuffered(video.buffered.end(video.buffered.length - 1));
      }
    };
    const onEnded = () => {
      setPlaying(false);
      if (isSeparateStreams && audioRef.current) {
        audioRef.current.pause();
      }
    };
    const onPlay = () => setPlaying(true);
    const onPause = () => setPlaying(false);
    const onError = (e: Event) => {
      console.error("[VideoPlayer] Video element error:", e, video.error);
      const mediaError = video.error;
      if (mediaError) {
        const errorMessages: Record<number, string> = {
          1: "MEDIA_ERR_ABORTED: The video playback was aborted",
          2: "MEDIA_ERR_NETWORK: A network error occurred while loading the video",
          3: "MEDIA_ERR_DECODE: An error occurred while decoding the video",
          4: "MEDIA_ERR_SRC_NOT_SUPPORTED: The video format is not supported or the URL is invalid",
        };
        setError(
          errorMessages[mediaError.code] ||
            `Unknown media error (code: ${mediaError.code})`,
        );
      }
    };
    const onLoadStart = () => console.log("[VideoPlayer] Video load started");
    const onLoadedMetadata = () =>
      console.log("[VideoPlayer] Video metadata loaded");
    const onCanPlay = () => console.log("[VideoPlayer] Video can play");

    video.addEventListener("timeupdate", onTimeUpdate);
    video.addEventListener("durationchange", onDurationChange);
    video.addEventListener("progress", onProgress);
    video.addEventListener("ended", onEnded);
    video.addEventListener("play", onPlay);
    video.addEventListener("pause", onPause);
    video.addEventListener("error", onError);
    video.addEventListener("loadstart", onLoadStart);
    video.addEventListener("loadedmetadata", onLoadedMetadata);
    video.addEventListener("canplay", onCanPlay);

    return () => {
      video.removeEventListener("timeupdate", onTimeUpdate);
      video.removeEventListener("durationchange", onDurationChange);
      video.removeEventListener("progress", onProgress);
      video.removeEventListener("ended", onEnded);
      video.removeEventListener("play", onPlay);
      video.removeEventListener("pause", onPause);
      video.removeEventListener("error", onError);
      video.removeEventListener("loadstart", onLoadStart);
      video.removeEventListener("loadedmetadata", onLoadedMetadata);
      video.removeEventListener("canplay", onCanPlay);
    };
  }, [streamInfo, syncAudio, isSeparateStreams]);

  // Set initial volume on video/audio elements
  useEffect(() => {
    if (isSeparateStreams) {
      if (videoRef.current) videoRef.current.volume = 0;
      if (audioRef.current) {
        audioRef.current.volume = volume;
        audioRef.current.muted = muted;
      }
    } else if (videoRef.current) {
      videoRef.current.volume = volume;
      videoRef.current.muted = muted;
    }
  }, [streamInfo, isSeparateStreams, volume, muted]);

  // Keyboard controls
  useEffect(() => {
    const onKeyDown = (e: KeyboardEvent) => {
      switch (e.key) {
        case " ":
        case "k":
          e.preventDefault();
          togglePlay();
          break;
        case "Escape":
          onClose();
          break;
        case "f":
          e.preventDefault();
          toggleFullscreen();
          break;
        case "m":
          e.preventDefault();
          toggleMute();
          break;
        case "ArrowLeft":
          e.preventDefault();
          skip(-10);
          break;
        case "ArrowRight":
          e.preventDefault();
          skip(10);
          break;
        case "ArrowUp":
          e.preventDefault();
          setVolume((v) => {
            const newV = Math.min(1, v + 0.1);
            if (videoRef.current) videoRef.current.volume = newV;
            if (audioRef.current) audioRef.current.volume = newV;
            return newV;
          });
          break;
        case "ArrowDown":
          e.preventDefault();
          setVolume((v) => {
            const newV = Math.max(0, v - 0.1);
            if (videoRef.current) videoRef.current.volume = newV;
            if (audioRef.current) audioRef.current.volume = newV;
            return newV;
          });
          break;
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [togglePlay, toggleFullscreen, toggleMute, skip, onClose]);

  // Auto-hide controls
  const resetControlsTimer = useCallback(() => {
    setShowControls(true);
    if (hideControlsTimeout.current) clearTimeout(hideControlsTimeout.current);
    hideControlsTimeout.current = setTimeout(() => {
      if (playing) setShowControls(false);
    }, 3000);
  }, [playing]);

  useEffect(() => {
    return () => {
      if (hideControlsTimeout.current)
        clearTimeout(hideControlsTimeout.current);
    };
  }, []);

  // Fullscreen change listener
  useEffect(() => {
    const onFSChange = () => setIsFullscreen(!!document.fullscreenElement);
    document.addEventListener("fullscreenchange", onFSChange);
    return () => document.removeEventListener("fullscreenchange", onFSChange);
  }, []);

  const videoSrc =
    selectedQuality?.url ||
    streamInfo?.videoUrl ||
    streamInfo?.combinedUrl ||
    "";
  const audioSrc = isSeparateStreams ? streamInfo?.audioUrl || "" : "";

  console.log(
    "[VideoPlayer] Render - videoSrc:",
    videoSrc ? videoSrc.substring(0, 100) + "..." : "empty",
  );
  console.log(
    "[VideoPlayer] Render - audioSrc:",
    audioSrc ? audioSrc.substring(0, 100) + "..." : "empty",
  );
  console.log("[VideoPlayer] Render - isSeparateStreams:", isSeparateStreams);

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/90 backdrop-blur-md"
      onClick={onClose}
    >
      <div
        ref={containerRef}
        className="relative bg-black rounded-2xl overflow-hidden shadow-2xl select-none"
        style={{
          width: isShort ? "min(92vw, 420px)" : "min(95vw, 1280px)",
          height: isShort ? "min(90vh, 760px)" : "min(90vh, 760px)",
        }}
        onClick={(e) => e.stopPropagation()}
        onMouseMove={resetControlsTimer}
        onMouseEnter={() => setShowControls(true)}
      >
        {/* Top bar */}
        <div
          className={`absolute top-0 left-0 right-0 z-30 transition-opacity duration-300 ${
            showControls ? "opacity-100" : "opacity-0 pointer-events-none"
          }`}
        >
          <div className="bg-gradient-to-b from-black/80 to-transparent px-4 pt-3 pb-8">
            <div className="flex items-center gap-3">
              <p className="text-white text-sm font-medium truncate flex-1">
                {streamInfo?.title || title}
              </p>
              {streamInfo?.uploader && (
                <span className="text-white/60 text-xs hidden sm:block">
                  {streamInfo.uploader}
                </span>
              )}
              <Button
                size="icon"
                variant="ghost"
                className="h-8 w-8 text-white/80 hover:text-white hover:bg-white/10"
                onClick={() => commands.openExternal(url)}
                title="Open in browser"
              >
                <ExternalLink className="w-4 h-4" />
              </Button>
              <Button
                size="icon"
                variant="ghost"
                className="h-8 w-8 text-white/80 hover:text-white hover:bg-white/10"
                onClick={onClose}
              >
                <X className="w-4 h-4" />
              </Button>
            </div>
          </div>
        </div>

        {/* Video area */}
        <div
          className="w-full h-full flex items-center justify-center bg-black cursor-pointer"
          onClick={togglePlay}
        >
          {loading ? (
            <div className="flex flex-col items-center gap-4">
              <Loader2 className="w-12 h-12 text-white/60 animate-spin" />
              <p className="text-white/60 text-sm">Loading stream...</p>
            </div>
          ) : error ? (
            <div className="flex flex-col items-center gap-4 px-8 text-center">
              <AlertCircle className="w-12 h-12 text-red-400" />
              <p className="text-white/80 text-sm font-medium">
                Failed to load video
              </p>
              <p className="text-white/50 text-xs max-w-md">{error}</p>
              <Button
                variant="secondary"
                size="sm"
                onClick={(e) => {
                  e.stopPropagation();
                  commands.openExternal(url);
                }}
              >
                <ExternalLink className="w-3.5 h-3.5 mr-2" />
                Open in browser
              </Button>
            </div>
          ) : (
            <>
              <video
                ref={videoRef}
                src={videoSrc}
                className={`w-full h-full ${isShort ? "object-contain" : "object-contain"}`}
                playsInline
                poster={streamInfo?.thumbnail}
                muted={isSeparateStreams ? true : muted}
              />
              {isSeparateStreams && audioSrc && (
                <audio ref={audioRef} src={audioSrc} preload="auto" />
              )}

              {/* Big play button when paused */}
              {!playing && !loading && (
                <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
                  <div className="w-20 h-20 rounded-full bg-white/20 backdrop-blur-sm flex items-center justify-center">
                    <Play className="w-10 h-10 text-white ml-1" />
                  </div>
                </div>
              )}
            </>
          )}
        </div>

        {/* Bottom controls */}
        {!loading && !error && (
          <div
            className={`absolute bottom-0 left-0 right-0 z-30 transition-opacity duration-300 ${
              showControls ? "opacity-100" : "opacity-0 pointer-events-none"
            }`}
          >
            <div className="bg-gradient-to-t from-black/90 to-transparent px-4 pb-3 pt-10">
              {/* Progress bar */}
              <div
                ref={progressRef}
                className="group relative h-1 hover:h-1.5 bg-white/20 rounded-full cursor-pointer mb-3 transition-all"
                onClick={handleSeek}
              >
                {/* Buffered */}
                <div
                  className="absolute inset-y-0 left-0 bg-white/30 rounded-full"
                  style={{
                    width:
                      duration > 0 ? `${(buffered / duration) * 100}%` : "0%",
                  }}
                />
                {/* Played */}
                <div
                  className="absolute inset-y-0 left-0 bg-red-500 rounded-full"
                  style={{
                    width:
                      duration > 0
                        ? `${(currentTime / duration) * 100}%`
                        : "0%",
                  }}
                />
                {/* Thumb */}
                <div
                  className="absolute top-1/2 -translate-y-1/2 w-3 h-3 bg-red-500 rounded-full shadow-lg opacity-0 group-hover:opacity-100 transition-opacity"
                  style={{
                    left:
                      duration > 0
                        ? `calc(${(currentTime / duration) * 100}% - 6px)`
                        : "0",
                  }}
                />
              </div>

              {/* Control buttons */}
              <div className="flex items-center gap-2">
                {/* Play/Pause */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    togglePlay();
                  }}
                  className="text-white hover:text-white/80 p-1"
                >
                  {playing ? (
                    <Pause className="w-5 h-5" />
                  ) : (
                    <Play className="w-5 h-5" />
                  )}
                </button>

                {/* Skip buttons */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    skip(-10);
                  }}
                  className="text-white/70 hover:text-white p-1"
                >
                  <SkipBack className="w-4 h-4" />
                </button>
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    skip(10);
                  }}
                  className="text-white/70 hover:text-white p-1"
                >
                  <SkipForward className="w-4 h-4" />
                </button>

                {/* Time display */}
                <span className="text-white/80 text-xs font-mono tabular-nums min-w-[90px]">
                  {formatTime(currentTime)} / {formatTime(duration)}
                </span>

                <div className="flex-1" />

                {/* Volume */}
                <div className="flex items-center gap-1 group/vol">
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleMute();
                    }}
                    className="text-white/70 hover:text-white p-1"
                  >
                    {muted || volume === 0 ? (
                      <VolumeX className="w-4 h-4" />
                    ) : (
                      <Volume2 className="w-4 h-4" />
                    )}
                  </button>
                  <div
                    className="w-0 group-hover/vol:w-20 overflow-hidden transition-all duration-200"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <div
                      className="w-20 h-1 bg-white/20 rounded-full cursor-pointer relative"
                      onClick={handleVolumeChange}
                    >
                      <div
                        className="absolute inset-y-0 left-0 bg-white rounded-full"
                        style={{ width: `${(muted ? 0 : volume) * 100}%` }}
                      />
                    </div>
                  </div>
                </div>

                {/* Quality selector */}
                {streamInfo?.qualities && streamInfo.qualities.length > 0 && (
                  <div className="relative">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setShowQualityMenu(!showQualityMenu);
                      }}
                      className="text-white/70 hover:text-white p-1 flex items-center gap-1"
                    >
                      <Settings className="w-4 h-4" />
                      {selectedQuality && (
                        <span className="text-[10px] font-medium">
                          {selectedQuality.height}p
                        </span>
                      )}
                    </button>

                    {showQualityMenu && (
                      <div
                        className="absolute bottom-full right-0 mb-2 bg-black/95 border border-white/10 rounded-lg py-1 min-w-[140px] shadow-xl"
                        onClick={(e) => e.stopPropagation()}
                      >
                        <p className="px-3 py-1 text-[10px] text-white/40 uppercase tracking-wider">
                          Quality
                        </p>
                        {streamInfo.qualities.map((q) => (
                          <button
                            key={q.formatId}
                            onClick={() => changeQuality(q)}
                            className={`w-full text-left px-3 py-1.5 text-xs hover:bg-white/10 transition-colors flex items-center justify-between ${
                              selectedQuality?.formatId === q.formatId
                                ? "text-red-400"
                                : "text-white/80"
                            }`}
                          >
                            <span>
                              {q.height}p
                              {q.fps > 30 ? ` ${Math.round(q.fps)}fps` : ""}
                            </span>
                            <span className="text-white/30 text-[10px] uppercase">
                              {q.ext}
                            </span>
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                )}

                {/* Download button */}
                {onDownload && (
                  <button
                    onClick={(e) => {
                      e.stopPropagation();
                      onDownload();
                    }}
                    className="text-white/70 hover:text-white p-1"
                    title="Download"
                  >
                    <Download className="w-4 h-4" />
                  </button>
                )}

                {/* PiP */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    togglePiP();
                  }}
                  className="text-white/70 hover:text-white p-1"
                  title="Picture in Picture"
                >
                  <PictureInPicture2 className="w-4 h-4" />
                </button>

                {/* Fullscreen */}
                <button
                  onClick={(e) => {
                    e.stopPropagation();
                    toggleFullscreen();
                  }}
                  className="text-white/70 hover:text-white p-1"
                >
                  {isFullscreen ? (
                    <Minimize className="w-4 h-4" />
                  ) : (
                    <Maximize className="w-4 h-4" />
                  )}
                </button>
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

function formatTime(seconds: number): string {
  if (!seconds || isNaN(seconds)) return "0:00";
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = Math.floor(seconds % 60);
  if (h > 0) {
    return `${h}:${m.toString().padStart(2, "0")}:${s.toString().padStart(2, "0")}`;
  }
  return `${m}:${s.toString().padStart(2, "0")}`;
}
