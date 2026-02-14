import fs from 'fs';
import path from 'path';
import https from 'https';
import { execSync } from 'child_process';

const JNI_LIBS_DIR = path.join('src-tauri', 'gen', 'android', 'app', 'src', 'main', 'jniLibs', 'arm64-v8a');
const FFMPEG_URL = 'https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-linuxarm64-gpl.tar.xz';
const YTDLP_URL = 'https://github.com/yt-dlp/yt-dlp/releases/download/2024.12.23/yt-dlp_linux_aarch64';

// Ensure directory exists
if (!fs.existsSync(JNI_LIBS_DIR)) {
    fs.mkdirSync(JNI_LIBS_DIR, { recursive: true });
    console.log(`Created directory: ${JNI_LIBS_DIR}`);
}

async function downloadFile(url, dest) {
    console.log(`Downloading ${url}...`);
    return new Promise((resolve, reject) => {
        const file = fs.createWriteStream(dest);
        https.get(url, (response) => {
            if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
                console.log(`Redirecting to ${response.headers.location}`);
                downloadFile(response.headers.location, dest).then(resolve).catch(reject);
                return;
            }
            if (response.statusCode !== 200) {
                reject(new Error(`Failed to download ${url}: ${response.statusCode}`));
                return;
            }
            response.pipe(file);
            file.on('finish', () => {
                file.close();
                resolve();
            });
        }).on('error', (err) => {
            fs.unlink(dest, () => { });
            reject(err);
        });
    });
}

function findFileRecursively(dir, filename) {
    const files = fs.readdirSync(dir);
    for (const file of files) {
        const fullPath = path.join(dir, file);
        try {
            const stat = fs.statSync(fullPath);
            if (stat.isDirectory()) {
                const found = findFileRecursively(fullPath, filename);
                if (found) return found;
            } else if (file === filename) {
                return fullPath;
            }
        } catch (e) {
            // Ignore
        }
    }
    return null;
}

async function main() {
    try {
        console.log('Checking yt-dlp...');
        const ytdlpPath = path.join(JNI_LIBS_DIR, 'libytdlp.so');
        if (fs.existsSync(ytdlpPath) && fs.statSync(ytdlpPath).size < 1000) {
            console.log('yt-dlp file is too small, deleting...');
            fs.unlinkSync(ytdlpPath);
        }

        if (!fs.existsSync(ytdlpPath)) {
            await downloadFile(YTDLP_URL, ytdlpPath);
            console.log('Downloaded yt-dlp to libytdlp.so');
        } else {
            console.log('yt-dlp already exists.');
        }

        console.log('Checking ffmpeg...');
        const ffmpegPath = path.join(JNI_LIBS_DIR, 'libffmpeg.so');
        // Check for invalid (small) ffmpeg file
        if (fs.existsSync(ffmpegPath) && fs.statSync(ffmpegPath).size < 1000) {
            console.log('libffmpeg.so is too small, deleting...');
            fs.unlinkSync(ffmpegPath);
        }

        if (!fs.existsSync(ffmpegPath)) {
            const tarPath = path.join(JNI_LIBS_DIR, 'ffmpeg.tar.xz');
            if (fs.existsSync(tarPath)) fs.unlinkSync(tarPath);

            await downloadFile(FFMPEG_URL, tarPath);
            console.log('Downloaded ffmpeg archive.');

            console.log('Extracting ffmpeg...');
            try {
                execSync(`tar -xf ${tarPath} -C ${JNI_LIBS_DIR}`);
            } catch (e) {
                console.error('Tar extraction failed. ' + e.message);
                throw e;
            }

            const ffmpegSrc = findFileRecursively(JNI_LIBS_DIR, 'ffmpeg');
            if (ffmpegSrc) {
                const dest = path.join(JNI_LIBS_DIR, 'libffmpeg.so');
                if (fs.existsSync(dest)) fs.unlinkSync(dest);
                fs.renameSync(ffmpegSrc, dest);
                console.log(`Renamed ${ffmpegSrc} to ${dest}`);

                if (fs.existsSync(tarPath)) fs.unlinkSync(tarPath);
            } else {
                console.log('Could not find ffmpeg binary in extracted files.');
                // List files for debugging
                const items = fs.readdirSync(JNI_LIBS_DIR);
                console.log('Items in dir:', items);
                process.exit(1);
            }
        } else {
            console.log('ffmpeg already exists.');
        }

        console.log('Android binaries setup complete.');
    } catch (error) {
        console.error('Error setting up Android binaries:', error);
        process.exit(1);
    }
}

main();
