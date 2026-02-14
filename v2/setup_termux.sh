#!/data/data/com.termux/files/usr/bin/bash
# Quick Setup Script for Termux
# One-command installation of YouTube Downloader

set -e

echo "🤖 YouTube Downloader - Termux Quick Setup"
echo "=========================================="
echo ""

# Update packages
echo "📦 Updating Termux packages..."
pkg update -y

# Install dependencies
echo "⬇️  Installing system dependencies..."
pkg install -y python ffmpeg git wget curl

# Install Python packages that require compilation
echo "� Installing numpy from Termux repository..."
pkg install -y python-numpy

# Install Python packages
echo "🐍 Installing Python dependencies..."
# Note: Do NOT use 'pip install --upgrade pip' in Termux!
# numpy is installed via pkg, not pip (requires compilation)
pip install Flask waitress yt-dlp moviepy colorama

# Setup storage
echo "📁 Setting up storage access..."
echo "⚠️  Please grant storage permission in the popup!"
termux-setup-storage

# Create download directory
mkdir -p ~/storage/downloads/YouTube

echo ""
echo "✅ Installation complete!"
echo ""
echo "To start the application, run:"
echo "  ./launcher_termux.sh"
echo ""
echo "Then choose option 1 to launch web interface"
echo ""
