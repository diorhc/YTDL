# YouTube Downloader - Mac/Linux Installation Guide

## 📋 Prerequisites

### macOS
```bash
# Install Homebrew (if not installed)
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"

# Install Python 3
brew install python3

# Install ffmpeg (recommended for video processing)
brew install ffmpeg
```

### Linux (Debian/Ubuntu)
```bash
# Update package list
sudo apt update

# Install Python 3 and pip
sudo apt install python3 python3-pip

# Install ffmpeg (recommended for video processing)
sudo apt install ffmpeg
```

### Linux (RedHat/CentOS/Fedora)
```bash
# Install Python 3 and pip
sudo yum install python3 python3-pip
# or for Fedora
sudo dnf install python3 python3-pip

# Install ffmpeg (recommended for video processing)
sudo yum install ffmpeg
# or for Fedora
sudo dnf install ffmpeg
```

## 🚀 Quick Start

1. **Clone or download the repository**
   ```bash
   cd /path/to/youtube-downloader
   ```

2. **Make the launcher script executable**
   ```bash
   chmod +x launcher.sh
   ```

3. **Run the launcher**
   ```bash
   ./launcher.sh
   ```

4. **First-time setup**
   - Select option `2` (Setup / Install Dependencies)
   - The script will automatically install all required Python packages
   - Follow the prompts

5. **Launch the web interface**
   - Select option `1` (Launch Web Interface)
   - Your browser will open automatically at http://localhost:5005
   - Start downloading videos!

## 🛠️ Manual Installation (Alternative)

If you prefer to install dependencies manually:

```bash
# Install Python dependencies
pip3 install -r requirements.txt

# Or using the system Python module
python3 -m pip install -r requirements.txt
```

## 📦 What Gets Installed

The script will install the following Python packages:
- **Flask** - Web framework for the interface
- **yt-dlp** - YouTube video downloader
- **moviepy** - Video processing library
- **pillow** - Image processing
- **waitress** - Production-ready WSGI server
- And other dependencies listed in `requirements.txt`

## 🎯 Usage

### Option 1: Using the Launcher Menu (Recommended)
```bash
./launcher.sh
```
Then select from the menu:
1. Launch Web Interface
2. Setup / Install Dependencies
3. Update Dependencies
4. Exit

### Option 2: Direct Web App Launch
```bash
python3 web_app.py
```
Then open http://localhost:5005 in your browser

## 🔧 Troubleshooting

### Permission Denied
```bash
chmod +x launcher.sh
```

### Python Command Not Found
Make sure Python 3 is installed:
```bash
python3 --version
```

### pip Command Not Found
Install pip:
```bash
# macOS
python3 -m ensurepip --upgrade

# Linux
sudo apt install python3-pip
```

### ffmpeg Not Found
While the downloader will work without ffmpeg, some features may be limited:
```bash
# macOS
brew install ffmpeg

# Linux (Debian/Ubuntu)
sudo apt install ffmpeg

# Linux (RedHat/CentOS)
sudo yum install ffmpeg
```

### Port 5005 Already in Use
Edit `web_app.py` and change the port number:
```python
# Find this line and change the port
app.run(host='0.0.0.0', port=5005)
```

## 🌐 Accessing from Other Devices

The web interface is accessible from other devices on your network:
1. Find your local IP address:
   ```bash
   # macOS
   ipconfig getifaddr en0
   
   # Linux
   hostname -I
   ```

2. On another device, open: `http://YOUR_IP_ADDRESS:5005`

## 📁 Download Location

Downloaded videos are saved to the `downloads/` folder in the same directory as the script.

## 🔄 Updating

To update dependencies to the latest versions:
```bash
./launcher.sh
# Select option 3 (Update Dependencies)
```

Or manually:
```bash
pip3 install --upgrade -r requirements.txt
```

## 🆘 Support

If you encounter any issues:
1. Make sure all prerequisites are installed
2. Run the setup option from the launcher menu
3. Check the terminal output for error messages
4. Ensure ffmpeg is installed for best compatibility

## 📝 Notes

- **macOS**: You may need to allow the app through System Preferences > Security & Privacy
- **Linux**: Some distributions may require `python3-venv` package
- **Firewall**: If accessing from other devices, ensure port 5005 is not blocked

## 🎬 Features

- Download videos in multiple quality options (4K, 1080p, 720p, etc.)
- Audio-only downloads (MP3)
- Real-time download progress tracking
- Download history
- Clean and intuitive web interface
- Multi-platform support (Windows, macOS, Linux)

Enjoy downloading! 🎉
