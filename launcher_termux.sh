#!/data/data/com.termux/files/usr/bin/bash

# YouTube Downloader Launcher for Termux (Android)
# This script provides a menu-driven interface for downloading YouTube videos on Android

set -e

# === CONFIGURABLE VARIABLES ===
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

PYTHON_CMD=""
PIP_CMD=""
FFMPEG_CMD=""

# === COLOR CODES ===
RESET='\033[0m'
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'

# === UTILITY FUNCTIONS ===

print_header() {
    clear
    echo -e "${CYAN}${BOLD}"
    echo "╔═══════════════════════════════════════════════╗"
    echo "║    🎬 YouTube Downloader for Termux 🤖       ║"
    echo "╚═══════════════════════════════════════════════╝${RESET}"
    echo ""
}

print_success() {
    echo -e "${GREEN}✓ $1${RESET}"
}

print_error() {
    echo -e "${RED}✗ $1${RESET}"
}

print_warning() {
    echo -e "${YELLOW}⚠ $1${RESET}"
}

print_info() {
    echo -e "${BLUE}ℹ $1${RESET}"
}

# === SYSTEM DETECTION ===

detect_python() {
    if command -v python3 &> /dev/null; then
        PYTHON_CMD="python3"
    elif command -v python &> /dev/null; then
        PYTHON_CMD="python"
    else
        print_error "Python not found. Installing Python..."
        pkg install python -y
        PYTHON_CMD="python"
    fi
    
    print_success "Python found: $PYTHON_CMD"
    $PYTHON_CMD --version
}

detect_pip() {
    if command -v pip3 &> /dev/null; then
        PIP_CMD="pip3"
    elif command -v pip &> /dev/null; then
        PIP_CMD="pip"
    else
        print_warning "pip not found. Installing pip..."
        pkg install python-pip -y
        PIP_CMD="pip"
    fi
    
    print_success "pip found: $PIP_CMD"
}

detect_ffmpeg() {
    if command -v ffmpeg &> /dev/null; then
        FFMPEG_CMD="ffmpeg"
        print_success "FFmpeg found: $FFMPEG_CMD"
        ffmpeg -version | head -n 1
    else
        print_warning "FFmpeg not found (optional but recommended)"
        echo -e "${YELLOW}FFmpeg is needed for:${RESET}"
        echo "  - Merging video and audio streams"
        echo "  - Converting to different formats"
        echo "  - Processing high-quality downloads"
    fi
}

# === INSTALLATION FUNCTIONS ===

install_termux_dependencies() {
    print_header
    echo -e "${BOLD}Installing Termux Dependencies...${RESET}\n"
    
    print_info "Updating package lists..."
    pkg update -y
    
    print_info "Installing required packages..."
    pkg install -y python ffmpeg git wget curl
    
    print_info "Installing Python packages that require compilation..."
    # numpy must be installed from Termux repo, not pip (requires compilation)
    pkg install -y python-numpy
    
    detect_python
    detect_pip
    detect_ffmpeg
    
    print_success "Termux dependencies installed!"
    echo ""
    read -p "Press Enter to continue..."
}

install_python_dependencies() {
    print_header
    echo -e "${BOLD}Installing Python Dependencies...${RESET}\n"
    
    detect_python
    detect_pip
    
    print_info "Installing Python packages from requirements.txt..."
    # Note: Do NOT upgrade pip in Termux - it will break python-pip package
    # Use 'pkg upgrade python-pip' instead if needed
    # Note: numpy is installed via pkg, not pip (requires compilation)
    
    # Install packages one by one to handle errors better
    print_info "Installing Flask..."
    $PIP_CMD install Flask waitress || print_warning "Flask installation had issues, continuing..."
    
    print_info "Installing yt-dlp..."
    $PIP_CMD install yt-dlp || print_warning "yt-dlp installation had issues, continuing..."
    
    print_info "Installing moviepy (may take a while)..."
    $PIP_CMD install moviepy || print_warning "moviepy installation had issues, continuing..."
    
    print_info "Installing colorama..."
    $PIP_CMD install colorama || print_warning "colorama installation had issues, continuing..."
    
    print_success "Python dependencies installed!"
    print_info "Note: numpy was installed via pkg (system package)"
    echo ""
    read -p "Press Enter to continue..."
}

setup_storage_access() {
    print_header
    echo -e "${BOLD}Setting up Storage Access...${RESET}\n"
    
    print_info "This will allow Termux to access your Android storage"
    print_info "You need to grant storage permission in the popup"
    echo ""
    termux-setup-storage
    
    print_success "Storage access configured!"
    print_info "Downloads will be saved to: $HOME/storage/downloads"
    echo ""
    read -p "Press Enter to continue..."
}

# === DOWNLOAD FUNCTIONS ===

get_download_directory() {
    # Default to Termux shared storage downloads folder
    if [ -d "$HOME/storage/downloads" ]; then
        echo "$HOME/storage/downloads/YouTube"
    elif [ -d "$HOME/storage/shared/Download" ]; then
        echo "$HOME/storage/shared/Download/YouTube"
    else
        echo "$SCRIPT_DIR/downloads"
    fi
}

launch_web_interface() {
    print_header
    echo -e "${BOLD}Launching Web Interface...${RESET}\n"
    
    detect_python
    
    # Get local IP address for access from other devices
    LOCAL_IP=$(ifconfig 2>/dev/null | grep -Eo 'inet (addr:)?([0-9]*\.){3}[0-9]*' | grep -Eo '([0-9]*\.){3}[0-9]*' | grep -v '127.0.0.1' | head -n1)
    
    print_info "Starting Flask server..."
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo -e "${BOLD}Access the web interface at:${RESET}"
    echo -e "  ${CYAN}• Local:${RESET}  http://localhost:5005"
    if [ -n "$LOCAL_IP" ]; then
        echo -e "  ${CYAN}• Network:${RESET} http://$LOCAL_IP:5005"
    fi
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
    echo ""
    echo -e "${YELLOW}Press Ctrl+C to stop the server${RESET}"
    echo ""
    
    $PYTHON_CMD web_app.py
}

download_video_cli() {
    print_header
    echo -e "${BOLD}Command Line Video Download${RESET}\n"
    
    detect_python
    
    read -p "Enter YouTube URL: " url
    if [ -z "$url" ]; then
        print_error "URL cannot be empty"
        read -p "Press Enter to continue..."
        return
    fi
    
    echo ""
    echo -e "${BOLD}Quality Options:${RESET}"
    echo "  1) Best quality available"
    echo "  2) 1080p"
    echo "  3) 720p"
    echo "  4) 480p"
    echo "  5) Audio only (MP3)"
    echo "  6) List available formats"
    echo ""
    read -p "Select option (1-6): " quality_option
    
    DOWNLOAD_DIR=$(get_download_directory)
    mkdir -p "$DOWNLOAD_DIR"
    
    case $quality_option in
        1)
            print_info "Downloading best quality..."
            $PYTHON_CMD youtube_downloader.py "$url" -o "$DOWNLOAD_DIR"
            ;;
        2)
            print_info "Downloading 1080p..."
            $PYTHON_CMD youtube_downloader.py "$url" -q 1080p -o "$DOWNLOAD_DIR"
            ;;
        3)
            print_info "Downloading 720p..."
            $PYTHON_CMD youtube_downloader.py "$url" -q 720p -o "$DOWNLOAD_DIR"
            ;;
        4)
            print_info "Downloading 480p..."
            $PYTHON_CMD youtube_downloader.py "$url" -q 480p -o "$DOWNLOAD_DIR"
            ;;
        5)
            print_info "Downloading audio only..."
            $PYTHON_CMD youtube_downloader.py "$url" --audio-only -o "$DOWNLOAD_DIR"
            ;;
        6)
            $PYTHON_CMD youtube_downloader.py "$url" --list-formats
            ;;
        *)
            print_error "Invalid option"
            ;;
    esac
    
    echo ""
    read -p "Press Enter to continue..."
}

# === MAIN MENU ===

show_menu() {
    print_header
    
    echo -e "${BOLD}Main Menu:${RESET}"
    echo ""
    echo -e "  ${GREEN}1)${RESET} Launch Web Interface ${CYAN}(Recommended)${RESET}"
    echo -e "  ${GREEN}2)${RESET} Download Video (Command Line)"
    echo ""
    echo -e "${BOLD}Setup:${RESET}"
    echo -e "  ${YELLOW}3)${RESET} Install Termux Dependencies"
    echo -e "  ${YELLOW}4)${RESET} Install Python Dependencies"
    echo -e "  ${YELLOW}5)${RESET} Setup Storage Access"
    echo ""
    echo -e "${BOLD}Information:${RESET}"
    echo -e "  ${BLUE}6)${RESET} System Information"
    echo -e "  ${BLUE}7)${RESET} Check Dependencies"
    echo ""
    echo -e "  ${RED}0)${RESET} Exit"
    echo ""
}

show_system_info() {
    print_header
    echo -e "${BOLD}System Information${RESET}\n"
    
    echo -e "${CYAN}Termux Version:${RESET}"
    if command -v termux-info &> /dev/null; then
        termux-info | head -n 5
    else
        print_warning "termux-info not available"
    fi
    
    echo ""
    echo -e "${CYAN}Python Version:${RESET}"
    if command -v python &> /dev/null; then
        python --version
    else
        print_warning "Python not installed"
    fi
    
    echo ""
    echo -e "${CYAN}Storage Status:${RESET}"
    df -h "$HOME" | tail -n 1
    
    echo ""
    echo -e "${CYAN}Download Directory:${RESET}"
    echo "$(get_download_directory)"
    
    echo ""
    read -p "Press Enter to continue..."
}

check_dependencies() {
    print_header
    echo -e "${BOLD}Checking Dependencies...${RESET}\n"
    
    # Check Python
    if command -v python &> /dev/null; then
        print_success "Python: $(python --version)"
    else
        print_error "Python: Not installed"
    fi
    
    # Check pip
    if command -v pip &> /dev/null; then
        print_success "pip: $(pip --version | cut -d' ' -f2)"
    else
        print_error "pip: Not installed"
    fi
    
    # Check FFmpeg
    if command -v ffmpeg &> /dev/null; then
        print_success "FFmpeg: $(ffmpeg -version | head -n1 | cut -d' ' -f3)"
    else
        print_warning "FFmpeg: Not installed (optional)"
    fi
    
    # Check Git
    if command -v git &> /dev/null; then
        print_success "Git: $(git --version | cut -d' ' -f3)"
    else
        print_warning "Git: Not installed"
    fi
    
    # Check Python packages
    echo ""
    echo -e "${BOLD}Python Packages:${RESET}"
    if command -v pip &> /dev/null; then
        for pkg in flask yt-dlp moviepy waitress; do
            if pip show "$pkg" &> /dev/null; then
                version=$(pip show "$pkg" | grep Version | cut -d' ' -f2)
                print_success "$pkg: $version"
            else
                print_warning "$pkg: Not installed"
            fi
        done
    fi
    
    # Check storage access
    echo ""
    echo -e "${BOLD}Storage Access:${RESET}"
    if [ -d "$HOME/storage" ]; then
        print_success "Storage access configured"
    else
        print_warning "Storage access not configured (run option 5)"
    fi
    
    echo ""
    read -p "Press Enter to continue..."
}

# === MAIN LOOP ===

main() {
    while true; do
        show_menu
        read -p "Select option: " choice
        
        case $choice in
            1)
                launch_web_interface
                ;;
            2)
                download_video_cli
                ;;
            3)
                install_termux_dependencies
                ;;
            4)
                install_python_dependencies
                ;;
            5)
                setup_storage_access
                ;;
            6)
                show_system_info
                ;;
            7)
                check_dependencies
                ;;
            0)
                print_header
                echo -e "${GREEN}Thank you for using YouTube Downloader!${RESET}"
                echo ""
                exit 0
                ;;
            *)
                print_error "Invalid option. Please try again."
                sleep 2
                ;;
        esac
    done
}

# Run main menu
main
