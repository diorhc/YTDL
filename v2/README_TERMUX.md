# 🤖 YTDL v2 — Установка для Android (Termux)

[![Platform](<https://img.shields.io/badge/platform-Android%20(Termux)-green.svg>)](https://f-droid.org/packages/com.termux/)
[![Python](https://img.shields.io/badge/python-3.8+-blue.svg)](https://www.python.org/)

## 📱 Требования

- Android 7.0+
- [Termux](https://f-droid.org/packages/com.termux/) из **F-Droid** (не из Google Play!)
- ~500 МБ свободного места
- Интернет-соединение

---

## 🚀 Быстрая установка

### 1. Установите Termux

Скачайте из [F-Droid](https://f-droid.org/packages/com.termux/) и откройте.

### 2. Обновите пакеты

```bash
pkg update -y && pkg upgrade -y
```

### 3. Скачайте YTDL

> **Не используйте** `git clone` — скачивайте готовый релиз.

```bash
pkg install -y wget tar
cd ~
wget https://github.com/diorhc/YTDL/releases/latest/download/YTDL-v2-unix.tar.gz
tar -xzf YTDL-v2-unix.tar.gz
cd YTDL-v2
chmod +x launcher_termux.sh setup_termux.sh
```

### 4. Запустите и установите зависимости

```bash
./launcher_termux.sh
```

В меню выберите по порядку:

1. **Опция 3** — Install Termux Dependencies (Python, FFmpeg, numpy)
2. **Опция 4** — Install Python Dependencies (Flask, yt-dlp, moviepy)
3. **Опция 5** — Setup Storage Access (доступ к файлам Android)

> **numpy** устанавливается через `pkg` (опция 3), а не через `pip`!

### 5. Запустите

```bash
./launcher_termux.sh
# Выберите 1 — Launch Web Interface
```

Откройте в браузере: **http://localhost:5005**

---

## 🎯 Использование

### Веб-интерфейс (рекомендуется)

```bash
./launcher_termux.sh
# Опция 1 → Launch Web Interface
```

- **Локально:** http://localhost:5005
- **По сети:** http://ВАШ_IP:5005 (показывается при запуске)

### Командная строка

```bash
# Лучшее качество
python youtube_downloader.py "https://youtu.be/VIDEO_ID"

# Конкретное качество
python youtube_downloader.py "https://youtu.be/VIDEO_ID" -q 720p

# Только аудио
python youtube_downloader.py "https://youtu.be/VIDEO_ID" --audio-only
```

---

## 📂 Расположение загрузок

По умолчанию файлы сохраняются в:

- `~/storage/downloads/YouTube` — если настроен Storage Access
- `~/storage/shared/Download/YouTube` — альтернатива
- `~/YTDL-v2/downloads` — резервная папка

Чтобы видеть файлы в файловом менеджере Android → используйте **Setup Storage Access** (опция 5).

---

## 🎨 Поддерживаемое качество

| Качество | Разрешение | FFmpeg        |
| -------- | ---------- | ------------- |
| 8K       | 7680×4320  | Требуется     |
| 4K       | 3840×2160  | Требуется     |
| 1440p    | 2560×1440  | Требуется     |
| 1080p    | 1920×1080  | —             |
| 720p     | 1280×720   | —             |
| 480p     | 854×480    | —             |
| 360p     | 640×360    | —             |
| Audio    | MP3        | Рекомендуется |

---

## 🔧 Важные команды

```bash
# Запуск
./launcher_termux.sh

# Обновление зависимостей
python -m pip install --upgrade -r requirements.txt

# ⚠️ НЕ обновляйте pip через pip!
# pip install --upgrade pip  ❌ Сломает python-pip
# Используйте: pkg upgrade python-pip  ✅

# Проверка зависимостей
./launcher_termux.sh  # → Опция 7

# Обновление Termux
pkg update && pkg upgrade
```

---

## 🔍 Решение проблем

| Проблема                      | Решение                                                    |
| ----------------------------- | ---------------------------------------------------------- |
| `Permission denied`           | `chmod +x launcher_termux.sh`                              |
| `Python not found`            | `pkg install python -y`                                    |
| `No module named 'flask'`     | Установите через меню → опция 4                            |
| `numpy` не ставится через pip | Используйте `pkg install python-numpy` (опция 3)           |
| Нет доступа к хранилищу       | Опция 5 (Setup Storage Access)                             |
| pip ошибка                    | `pkg upgrade python-pip` (не `pip install --upgrade pip`!) |

---

## 🌐 Доступ по сети

Чтобы открыть веб-интерфейс на другом устройстве в той же Wi-Fi сети:

1. Запустите веб-интерфейс (опция 1)
2. Скрипт покажет сетевой адрес: `http://192.168.x.x:5005`
3. Откройте этот адрес на другом устройстве

---

[⬅️ Назад к README](README.md)
