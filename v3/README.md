<div align="center">

![Demo (screenshot)](https://i.imgur.com/Y8KGMg6.png)

# YTDL v3

### Next-Gen Cross-Platform Media Engine

[![Tauri v2](https://img.shields.io/badge/Tauri-2.0-FFC131?style=for-the-badge&logo=tauri&logoColor=white)](https://tauri.app/)
[![React 19](https://img.shields.io/badge/React-19.0-61DAFB?style=for-the-badge&logo=react&logoColor=white)](https://react.dev/)
[![Tailwind](https://img.shields.io/badge/Tailwind-3.4-38B2AC?style=for-the-badge&logo=tailwind-css&logoColor=white)](https://tailwindcss.com/)
[![Rust Engine](https://img.shields.io/badge/Rust-1.75+-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)

**The highly optimized core of YTDL.** Leveraging the power of Tauri v2 and React 19 to deliver a blazing-fast, secure, and beautiful media experience.

---

</div>

## 🌌 Overview

This directory contains the source code for **YTDL v3**, a complete rewrite focused on performance, modularity, and cross-platform native feel. It integrates cutting-edge web technologies with the safety and speed of Rust.

## 🚀 Key Technologies

- **Frontend Core**: [React 19](https://react.dev/) with [Concurrent Mode](https://react.dev/blog/2024/12/05/react-19) for fluid UI.
- **Styling Layer**: [Tailwind CSS](https://tailwindcss.com/) & [Radix UI](https://www.radix-ui.com/) for accessible, premium-feel components.
- **Native Bridge**: [Tauri v2](https://v2.tauri.app/) for a 90% reduction in bundle size compared to Electron.
- **State Management**: [Jotai](https://jotai.org/) (Atomic State) for efficient, granular updates.
- **Media Engine**: [yt-dlp](https://github.com/yt-dlp/yt-dlp) & [ffmpeg](https://ffmpeg.org/) for robust media processing.

## 📂 Project Structure

```bash
v3/
├── src/               # React Frontend (Typescript)
│   ├── components/    # Reusable UI elements
│   ├── pages/         # Dashboard, RSS, Transcribe, etc.
│   └── hooks/         # Custom business logic
├── src-tauri/         # Rust Backend
│   ├── src/           # System commands, DB, and Core logic
│   └── capabilities/  # Tauri permission profiles
├── public/            # Static assets
└── scripts/           # Native build & deployment helpers
```

## 🛠 Development Workflow

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install)
- [Node.js](https://nodejs.org/) (v18+)
- [Tauri CLI](https://v2.tauri.app/start/prerequisites/)

### Setup & Run

```bash
# Install dependencies
npm install

# Start development environment
npm run tauri dev
```

### Build for Production

```bash
# Build desktop app
npm run tauri build

# Build Android APK
npm run tauri android build
```

## 📥 Установка (для пользователей)

> **Не используйте** `git clone` — скачивайте готовые релизы.

Перейдите на [**Releases**](https://github.com/diorhc/YTDL/releases) и скачайте установщик для вашей платформы:

| Платформа  | Формат               |
| ---------- | -------------------- |
| 🖥️ Windows | `.msi` / `.exe`      |
| 🐧 Linux   | `.deb` / `.AppImage` |
| 🍎 macOS   | `.dmg`               |
| 🤖 Android | `.apk`               |

## ✨ Advanced Features

| Feature              | Description                                | Status |
| :------------------- | :----------------------------------------- | :----: |
| **Whisper AI**       | High-precision audio-to-text transcription |   ✅   |
| **RSS Auto-Sync**    | Background fetching of media feeds         |   ✅   |
| **Mobile Optimized** | Full responsive support for iOS/Android    |   ✅   |

---

<div align="center">

### 🎬 Made with ❤️ for the YouTube downloading community

⭐ Star this repository if you find it useful!

[⬅️ Back to Project Root](../README.md)

</div>
