import fs from "fs";
import path from "path";
import https from "https";

const JNI_LIBS_DIR = path.join(
  "src-tauri",
  "gen",
  "android",
  "app",
  "src",
  "main",
  "jniLibs",
  "arm64-v8a",
);

// ─── Binary URLs ──────────────────────────────────────────────────────────────
//  ffmpeg/ffprobe: eugeneware/ffmpeg-static builds for linux-arm64 are compiled
//  with musl and are truly statically linked, so they run on Android (Bionic).
//
//  yt-dlp: the standard "yt-dlp_linux_aarch64" is a glibc-dynamically-linked
//  Nuitka build — it CANNOT run on Android.  We use the musl-static build
//  "yt-dlp_linux_musl_aarch64" instead, which is truly statically linked and
//  will execute on Android without glibc.
// ─────────────────────────────────────────────────────────────────────────────
const FFMPEG_URL =
  "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffmpeg-linux-arm64";
const FFPROBE_URL =
  "https://github.com/eugeneware/ffmpeg-static/releases/latest/download/ffprobe-linux-arm64";
// musllinux build (Android/Bionic compatible — statically linked, no glibc dependency)
const YTDLP_URL =
  "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_musllinux_aarch64";
// glibc fallback (will NOT run on Android but kept for non-Android linux)
const YTDLP_GLIBC_URL =
  "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64";

// Ensure directory exists
if (!fs.existsSync(JNI_LIBS_DIR)) {
  fs.mkdirSync(JNI_LIBS_DIR, { recursive: true });
  console.log(`Created directory: ${JNI_LIBS_DIR}`);
}

async function downloadFile(url, dest) {
  console.log(`Downloading ${url}...`);
  return new Promise((resolve, reject) => {
    const file = fs.createWriteStream(dest);
    const request = (downloadUrl) => {
      https
        .get(downloadUrl, (response) => {
          if (
            response.statusCode >= 300 &&
            response.statusCode < 400 &&
            response.headers.location
          ) {
            console.log(`Redirecting to ${response.headers.location}`);
            file.close();
            fs.unlinkSync(dest);
            downloadFile(response.headers.location, dest)
              .then(resolve)
              .catch(reject);
            return;
          }
          if (response.statusCode !== 200) {
            reject(
              new Error(
                `Failed to download ${downloadUrl}: HTTP ${response.statusCode}`,
              ),
            );
            return;
          }

          let downloaded = 0;
          const totalSize = parseInt(
            response.headers["content-length"] || "0",
            10,
          );

          response.on("data", (chunk) => {
            downloaded += chunk.length;
            if (totalSize > 0) {
              const pct = ((downloaded / totalSize) * 100).toFixed(1);
              process.stdout.write(
                `\r  Progress: ${pct}% (${(downloaded / 1024 / 1024).toFixed(1)} MB)`,
              );
            }
          });

          response.pipe(file);
          file.on("finish", () => {
            file.close();
            console.log(""); // newline after progress
            resolve();
          });
        })
        .on("error", (err) => {
          fs.unlink(dest, () => {});
          reject(err);
        });
    };
    request(url);
  });
}

function looksLikeGlibcAarch64Elf(filePath) {
  try {
    if (!fs.existsSync(filePath)) return false;
    const buf = fs.readFileSync(filePath);
    const sample = buf.toString("latin1");
    // Typical glibc dynamic loader for aarch64 Linux.
    // If present, this binary will not run on Android/Bionic.
    return sample.includes("/lib/ld-linux-aarch64.so.1");
  } catch {
    return false;
  }
}

async function main() {
  try {
    // --- yt-dlp ---
    console.log("\n=== Setting up yt-dlp ===");
    const ytdlpPath = path.join(JNI_LIBS_DIR, "libytdlp.so");

    // Remove invalid/small files
    if (fs.existsSync(ytdlpPath) && fs.statSync(ytdlpPath).size < 1000) {
      console.log("yt-dlp file is too small, deleting...");
      fs.unlinkSync(ytdlpPath);
    }

    // If a stale glibc-linked yt-dlp is present, force re-download musl build.
    if (looksLikeGlibcAarch64Elf(ytdlpPath)) {
      console.log(
        "Detected glibc-linked yt-dlp (not Android-compatible), replacing with musl build...",
      );
      fs.unlinkSync(ytdlpPath);
    }

    if (!fs.existsSync(ytdlpPath)) {
      // Try musl-static build first (Android/Bionic compatible)
      console.log("Trying musl-static yt-dlp build (Android compatible)...");
      try {
        await downloadFile(YTDLP_URL, ytdlpPath);
        const size = fs.statSync(ytdlpPath).size;
        if (size < 1000) {
          throw new Error(
            "Downloaded file too small – likely a redirect/placeholder",
          );
        }
        console.log(
          `Downloaded yt-dlp (musl-static) as libytdlp.so (${(size / 1024 / 1024).toFixed(1)} MB)`,
        );
        console.log(
          "✓ musl-static build is Android/Bionic compatible (no glibc dependency)",
        );
      } catch (e) {
        console.warn(`musl-static yt-dlp not available: ${e.message}`);
        console.warn(
          "Falling back to glibc build (will NOT run on Android)...",
        );
        if (fs.existsSync(ytdlpPath)) fs.unlinkSync(ytdlpPath);
        await downloadFile(YTDLP_GLIBC_URL, ytdlpPath);
        const size = fs.statSync(ytdlpPath).size;
        if (size < 1000) {
          console.error(
            "ERROR: Downloaded yt-dlp is too small, likely a redirect page",
          );
          process.exit(1);
        }
        console.log(
          `Downloaded yt-dlp (glibc) as libytdlp.so (${(size / 1024 / 1024).toFixed(1)} MB)`,
        );
        console.warn(
          "⚠ This glibc build requires libglibc which is NOT present on Android.",
          "On Android, yt-dlp will only work via Termux or Python wrapper.",
        );
      }
    } else {
      const size = fs.statSync(ytdlpPath).size;
      console.log(
        `yt-dlp already exists (${(size / 1024 / 1024).toFixed(1)} MB)`,
      );
    }

    // --- ffmpeg ---
    console.log("\n=== Setting up ffmpeg ===");
    const ffmpegPath = path.join(JNI_LIBS_DIR, "libffmpeg.so");

    if (fs.existsSync(ffmpegPath) && fs.statSync(ffmpegPath).size < 1000) {
      console.log("libffmpeg.so is too small, deleting...");
      fs.unlinkSync(ffmpegPath);
    }

    if (!fs.existsSync(ffmpegPath)) {
      await downloadFile(FFMPEG_URL, ffmpegPath);
      const size = fs.statSync(ffmpegPath).size;
      console.log(
        `Downloaded ffmpeg as libffmpeg.so (${(size / 1024 / 1024).toFixed(1)} MB)`,
      );

      if (size < 1000) {
        console.error("ERROR: Downloaded ffmpeg is too small");
        process.exit(1);
      }
    } else {
      const size = fs.statSync(ffmpegPath).size;
      console.log(
        `ffmpeg already exists (${(size / 1024 / 1024).toFixed(1)} MB)`,
      );
    }

    // --- ffprobe ---
    console.log("\n=== Setting up ffprobe ===");
    const ffprobePath = path.join(JNI_LIBS_DIR, "libffprobe.so");

    if (fs.existsSync(ffprobePath) && fs.statSync(ffprobePath).size < 1000) {
      console.log("libffprobe.so is too small, deleting...");
      fs.unlinkSync(ffprobePath);
    }

    if (!fs.existsSync(ffprobePath)) {
      await downloadFile(FFPROBE_URL, ffprobePath);
      const size = fs.statSync(ffprobePath).size;
      console.log(
        `Downloaded ffprobe as libffprobe.so (${(size / 1024 / 1024).toFixed(1)} MB)`,
      );

      if (size < 1000) {
        console.error("ERROR: Downloaded ffprobe is too small");
        process.exit(1);
      }
    } else {
      const size = fs.statSync(ffprobePath).size;
      console.log(
        `ffprobe already exists (${(size / 1024 / 1024).toFixed(1)} MB)`,
      );
    }

    // Clean up any leftover extraction directories
    const entries = fs.readdirSync(JNI_LIBS_DIR);
    for (const entry of entries) {
      const fullPath = path.join(JNI_LIBS_DIR, entry);
      if (fs.statSync(fullPath).isDirectory()) {
        console.log(`Cleaning up leftover directory: ${entry}`);
        fs.rmSync(fullPath, { recursive: true, force: true });
      }
    }

    // Summary
    console.log("\n=== Android binaries setup complete ===");
    const finalEntries = fs.readdirSync(JNI_LIBS_DIR);
    for (const entry of finalEntries) {
      const fullPath = path.join(JNI_LIBS_DIR, entry);
      const size = fs.statSync(fullPath).size;
      console.log(`  ${entry}: ${(size / 1024 / 1024).toFixed(1)} MB`);
    }
  } catch (error) {
    console.error("Error setting up Android binaries:", error);
    process.exit(1);
  }
}

main();
