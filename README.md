<div align="center">

# YTDL

**Мощный загрузчик видео с YouTube, VK, Dzen, Rutube и других платформ.**

[![License](https://img.shields.io/badge/license-MIT-blue.svg?style=for-the-badge)](v2/LICENSE)
[![GitHub Release](https://img.shields.io/github/v/release/diorhc/YTDL?style=for-the-badge)](https://github.com/diorhc/YTDL/releases)

</div>

---

## 📦 Два варианта

Этот проект имеет две версии. Выберите подходящую и скачайте со страницы [**Releases**](https://github.com/diorhc/YTDL/releases).

<table>
<tr>
<td width="50%" valign="top">

### YTDL v2 — Web

![v2 Screenshot](https://i.imgur.com/pAeRvcz.png)

**Python + Flask веб-интерфейс**

- Работает в браузере (localhost:5005)
- Windows, macOS, Linux, Android (Termux)
- Не требует установки — запустите лаунчер
- Командная строка + веб-интерфейс

| Платформа  | Файл                  |
| ---------- | --------------------- |
| 🖥️ Windows | `YTDL-v2-windows.zip` |
| 🐧 Linux   | `YTDL-v2-unix.tar.gz` |
| 🍎 macOS   | `YTDL-v2-unix.tar.gz` |
| 🤖 Android | `YTDL-v2-unix.tar.gz` |

📖 [Документация v2](v2/README.md)

</td>
<td width="50%" valign="top">

### YTDL v3 — Desktop App

![v3 Screenshot](https://i.imgur.com/Y8KGMg6.png)

**Tauri v2 + React 19 + Rust**

- Нативное десктопное приложение
- Windows, macOS, Linux, Android
- Whisper AI транскрипция
- RSS авто-синхронизация

| Платформа  | Файл                 |
| ---------- | -------------------- |
| 🖥️ Windows | `.msi` / `.exe`      |
| 🐧 Linux   | `.deb` / `.AppImage` |
| 🍎 macOS   | `.dmg`               |
| 🤖 Android | `.apk`               |

📖 [Документация v3](v3/README.md)

</td>
</tr>
</table>

---

## 📥 Установка

> **Не используйте** `git clone` для установки. Скачивайте готовые релизы.

### Шаг 1: Перейдите на [Releases](https://github.com/diorhc/YTDL/releases)

### Шаг 2: Скачайте нужную версию

- **v2** — `.zip` (Windows) или `.tar.gz` (Linux, macOS, Android)
- **v3** — установщик для вашей ОС

### Шаг 3: Распакуйте и запустите

**v2 (Windows):**

```
Распакуйте YTDL-v2-windows.zip → запустите launcher.bat → выберите Setup → Launch
```

**v2 (Linux/macOS):**

```bash
tar -xzf YTDL-v2-unix.tar.gz && cd YTDL-v2
chmod +x launcher.sh && ./launcher.sh
```

**v3:** Запустите установщик для вашей платформы.

---

## ✨ Возможности

| Функция                 | v2  | v3  |
| ----------------------- | :-: | :-: |
| Загрузка видео до 8K    | ✅  | ✅  |
| Аудио MP3               | ✅  | ✅  |
| Веб-интерфейс           | ✅  |  —  |
| Нативное приложение     |  —  | ✅  |
| Обрезка видео           | ✅  | ✅  |
| Whisper AI транскрипция |  —  | ✅  |
| RSS авто-синхронизация  |  —  | ✅  |
| Android (Termux)        | ✅  |  —  |
| Android (APK)           |  —  | ✅  |
| Не требует установки    | ✅  |  —  |

---

## 🔒 Безопасность

Все данные обрабатываются локально. Никакой телеметрии.

---

<div align="center">

Made with ❤️ by [diorhc](https://github.com/diorhc)

⭐ Поставьте звезду, если проект полезен!

</div>
