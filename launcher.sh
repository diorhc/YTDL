#!/bin/bash

# YouTube Downloader Launcher for Mac/Linux
# This script provides a menu-driven interface for downloading YouTube videos

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
    echo "╔═══════════════════════════════════════════════════════════════════╗"
    echo "║                                                                   ║"
    echo "║              🎬 YOUTUBE DOWNLOADER LAUNCHER 🎬                    ║"
    echo "║                                                                   ║"
    echo "╚═══════════════════════════════════════════════════════════════════╝"
    echo -e "${RESET}"
}

print_animated_header() {
    print_header
}

print_menu() {
    echo -e "${BLUE}${BOLD}"
    echo "╔═══════════════════════════════════════════════════════════════════╗"
    echo "║                         MAIN MENU                                 ║"
    echo "╠═══════════════════════════════════════════════════════════════════╣"
    echo "║                                                                   ║"
    echo "║  1 🌐 Launch Web Interface                                   ║"
    echo "║                                                                   ║"
    echo "║  2 ⚙️  Setup / Install Dependencies                          ║"
    echo "║                                                                   ║"
    echo "║  3 🔄 Update Dependencies                                    ║"
    echo "║                                                                   ║"
    echo "║  4 🚪 Exit                                                   ║"
    echo "║                                                                   ║"
    echo "╚═══════════════════════════════════════════════════════════════════╝"
    echo -e "${RESET}"
}

print_section() {
    echo -e "${CYAN}${BOLD}═══ $1 ═══${RESET}"
}

print_info() {
    echo -e "${BLUE}[INFO]${RESET} $1"
}

print_success() {
    echo -e "${GREEN}[✓]${RESET} $1"
}

print_error() {
    echo -e "${RED}[✗]${RESET} $1"
}

print_warning() {
    echo -e "${YELLOW}[!]${RESET} $1"
}

# === DETECT SYSTEM ===

detect_system() {
    OS_TYPE=$(uname -s)
    case "$OS_TYPE" in
        Darwin*)
            SYSTEM="macOS"
            ;;
        Linux*)
            SYSTEM="Linux"
            ;;
        *)
            SYSTEM="Unknown"
            ;;
    esac
}

# === FIND PYTHON ===

find_python() {
    # Try to find Python 3
    if command -v python3 &>/dev/null; then
        PYTHON_CMD="python3"
    elif command -v python &>/dev/null; then
        # Check if it's Python 3
        py_version=$($PYTHON_CMD --version 2>&1 | grep -oP '(?<=Python )\d')
        if [ "$py_version" = "3" ]; then
            PYTHON_CMD="python"
        else
            PYTHON_CMD=""
        fi
    else
        PYTHON_CMD=""
    fi

    if [ -n "$PYTHON_CMD" ]; then
        PIP_CMD="$PYTHON_CMD -m pip"
        return 0
    else
        return 1
    fi
}

# === CHECK DEPENDENCIES ===

check_dependencies() {
    local all_ok=true

    print_section "Checking Dependencies"

    # Check Python
    if find_python; then
        py_version=$($PYTHON_CMD --version 2>&1)
        print_success "Python found: $py_version"
    else
        print_error "Python 3 not found!"
        all_ok=false
    fi

    # Check pip
    if [ -n "$PIP_CMD" ] && $PIP_CMD --version &>/dev/null; then
        pip_version=$($PIP_CMD --version 2>&1 | head -n1)
        print_success "pip found: $pip_version"
    else
        print_error "pip not found!"
        all_ok=false
    fi

    # Check ffmpeg
    if command -v ffmpeg &>/dev/null; then
        ffmpeg_version=$(ffmpeg -version 2>&1 | head -n1)
        print_success "ffmpeg found: $ffmpeg_version"
        FFMPEG_CMD="ffmpeg"
    else
        print_warning "ffmpeg not found! Some features may not work."
        print_info "Install with: brew install ffmpeg (macOS) or apt install ffmpeg (Linux)"
    fi

    if [ "$all_ok" = false ]; then
        return 1
    else
        return 0
    fi
}

# === INSTALL DEPENDENCIES ===

install_dependencies() {
    print_section "Installing Python Dependencies"

    if [ ! -f "requirements.txt" ]; then
        print_error "requirements.txt not found!"
        return 1
    fi

    if [ -z "$PYTHON_CMD" ]; then
        print_error "Python not found! Please install Python 3 first."
        print_info "Visit https://www.python.org/downloads/"
        return 1
    fi

    print_info "Installing packages from requirements.txt..."
    $PIP_CMD install -r requirements.txt

    if [ $? -eq 0 ]; then
        print_success "Dependencies installed successfully!"
        return 0
    else
        print_error "Failed to install dependencies!"
        return 1
    fi
}

# === UPDATE DEPENDENCIES ===

update_dependencies() {
    print_section "Checking for Dependency Updates"

    if [ -z "$PYTHON_CMD" ]; then
        print_error "Python not found! Please run Setup first."
        return 1
    fi

    print_info "Checking for outdated packages..."
    outdated=$($PIP_CMD list --outdated 2>/dev/null)

    if [ -z "$outdated" ]; then
        print_success "All packages are up to date!"
    else
        echo -e "\n${YELLOW}Outdated packages:${RESET}"
        echo "$outdated"
        echo ""
        read -p "Do you want to update all packages? (y/n): " choice
        if [ "$choice" = "y" ] || [ "$choice" = "Y" ]; then
            print_info "Updating packages..."
            $PIP_CMD install --upgrade -r requirements.txt
            if [ $? -eq 0 ]; then
                print_success "Packages updated successfully!"
            else
                print_error "Failed to update packages!"
            fi
        else
            print_info "Update cancelled."
        fi
    fi
}

# === LAUNCH WEB APP ===

launch_web() {
    print_section "Launching Web Interface"

    if [ -z "$PYTHON_CMD" ]; then
        print_error "Python not found! Please run Setup first."
        return 1
    fi

    if [ ! -f "web_app.py" ]; then
        print_error "web_app.py not found!"
        return 1
    fi

    print_info "Starting web server..."
    print_success "Web interface will be available at: http://localhost:5005"
    
    # Open browser (platform-specific)
    sleep 2
    case "$SYSTEM" in
        macOS)
            open "http://localhost:5005" &>/dev/null &
            ;;
        Linux)
            if command -v xdg-open &>/dev/null; then
                xdg-open "http://localhost:5005" &>/dev/null &
            elif command -v gnome-open &>/dev/null; then
                gnome-open "http://localhost:5005" &>/dev/null &
            fi
            ;;
    esac

    echo ""
    print_warning "Press Ctrl+C to stop the server and return to menu"
    echo ""

    $PYTHON_CMD web_app.py
}

# === SETUP ===

setup() {
    print_animated_header
    print_section "Setup / Installation"

    detect_system
    print_info "System detected: $SYSTEM"
    echo ""

    if ! check_dependencies; then
        echo ""
        print_warning "Some dependencies are missing!"
        
        if [ -z "$PYTHON_CMD" ]; then
            print_error "Python 3 is required but not found."
            case "$SYSTEM" in
                macOS)
                    print_info "Install with: brew install python3"
                    print_info "Or download from: https://www.python.org/downloads/"
                    ;;
                Linux)
                    print_info "Install with: sudo apt install python3 python3-pip (Debian/Ubuntu)"
                    print_info "            or: sudo yum install python3 python3-pip (RedHat/CentOS)"
                    ;;
            esac
            echo ""
            read -p "Press Enter to return to menu..."
            return 1
        fi
    fi

    echo ""
    if [ -f "requirements.txt" ]; then
        read -p "Install/Update Python dependencies? (y/n): " choice
        if [ "$choice" = "y" ] || [ "$choice" = "Y" ]; then
            install_dependencies
        fi
    fi

    echo ""
    print_success "Setup complete!"
    echo ""
    read -p "Press Enter to return to menu..."
}

# === MAIN MENU ===

main_menu() {
    while true; do
        print_animated_header
        print_menu

        read -p "  Enter choice (1-4): " choice
        echo ""

        case $choice in
            1)
                launch_web
                ;;
            2)
                setup
                ;;
            3)
                print_header
                update_dependencies
                echo ""
                read -p "Press Enter to return to menu..."
                ;;
            4)
                print_header
                print_success "Thank you for using YouTube Downloader!"
                echo ""
                exit 0
                ;;
            *)
                print_error "Invalid choice! Please enter a number from 1 to 4."
                sleep 2
                ;;
        esac
    done
}

# === ENTRY POINT ===

detect_system
find_python

main_menu
