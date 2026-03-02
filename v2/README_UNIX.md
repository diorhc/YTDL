# YTDL v2 — Установка для Linux / macOS

## 📋 Требования

- Python 3.8+
- pip
- FFmpeg (рекомендуется)

### Установка зависимостей ОС

<details>
<summary><b>macOS</b></summary>

```bash
# Homebrew (если не установлен)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

brew install python3 ffmpeg
```

</details>

<details>
<summary><b>Debian / Ubuntu</b></summary>

```bash
sudo apt update
sudo apt install -y python3 python3-pip ffmpeg
```

</details>

<details>
<summary><b>Fedora / RHEL</b></summary>

```bash
sudo dnf install -y python3 python3-pip ffmpeg
```

</details>

---

## 📥 Установка

> **Не используйте** `git clone` — скачивайте готовый релиз.

1. Перейдите на [**Releases**](https://github.com/diorhc/YTDL/releases)
2. Скачайте **`YTDL-v2-unix.tar.gz`**
3. Распакуйте и запустите:

```bash
tar -xzf YTDL-v2-unix.tar.gz
cd YTDL-v2
chmod +x launcher.sh
./launcher.sh
```

4. В меню выберите **2 (Setup)** — установятся Python-зависимости
5. Выберите **1 (Launch)** — откроется веб-интерфейс

Или установите зависимости вручную:

```bash
pip3 install -r requirements.txt
python3 web_app.py
```

---

## 🎯 Использование

### Через лаунчер (рекомендуется)

```bash
./launcher.sh
```

Меню:

1. **Launch Web Interface** — запуск сервера
2. **Setup / Install** — установка зависимостей
3. **Update** — обновление пакетов
4. **Exit**

### Напрямую

```bash
# Веб-интерфейс
python3 web_app.py

# Командная строка
python3 youtube_downloader.py "https://youtu.be/VIDEO_ID" -q 1080p
```

Откройте http://localhost:5005 в браузере.

---

## 🌐 Доступ с других устройств

```bash
# Узнайте IP:
# macOS
ipconfig getifaddr en0
# Linux
hostname -I
```

На другом устройстве откройте: `http://ВАШ_IP:5005`

---

## 🔧 Решение проблем

| Проблема             | Решение                                             |
| -------------------- | --------------------------------------------------- |
| `Permission denied`  | `chmod +x launcher.sh`                              |
| `python3: not found` | Установите Python 3 через пакетный менеджер         |
| `pip: not found`     | `python3 -m ensurepip --upgrade`                    |
| `ffmpeg: not found`  | `brew install ffmpeg` или `sudo apt install ffmpeg` |
| Порт 5005 занят      | Измените `FLASK_PORT` окружающей переменной         |

---

## 📁 Загрузки

Файлы сохраняются в папку `downloads/` рядом со скриптом.

## 🔄 Обновление

```bash
./launcher.sh
# Выберите 3 (Update)
```

Или вручную: `pip3 install --upgrade -r requirements.txt`

---

[⬅️ Назад к README](README.md)
