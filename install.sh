#!/bin/bash

set -e

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "==================================="
echo "Infraware Terminal Backend Setup"
echo "==================================="
echo ""
echo -e "${YELLOW}WARNING: This script will install packages using pip with --break-system-packages flag.${NC}"
echo -e "${YELLOW}This may override system-managed packages. Use with caution.${NC}"
echo ""

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
}

# Function to check Python package
python_package_exists() {
    python3 -c "import $1" >/dev/null 2>&1
}

# Function to check if uv package is installed
uv_package_exists() {
    uv pip list 2>/dev/null | grep -q "^$1 "
}

# Check Python 3
echo -n "Checking Python 3... "
if command_exists python3; then
    PYTHON_VERSION=$(python3 --version | awk '{print $2}')
    echo -e "${GREEN}Found${NC} (version $PYTHON_VERSION)"

    # Check if Python 3.12+
    PYTHON_MAJOR=$(echo $PYTHON_VERSION | cut -d. -f1)
    PYTHON_MINOR=$(echo $PYTHON_VERSION | cut -d. -f2)
    if [ "$PYTHON_MAJOR" -lt 3 ] || ([ "$PYTHON_MAJOR" -eq 3 ] && [ "$PYTHON_MINOR" -lt 12 ]); then
        echo -e "${YELLOW}Warning: Python 3.12+ recommended, found $PYTHON_VERSION${NC}"
    fi
else
    echo -e "${RED}Not found${NC}"
    echo "Please install Python 3.12 or higher"
    echo "Visit: https://www.python.org/downloads/"
    exit 1
fi

# Check pip
echo -n "Checking pip... "
if command_exists pip3; then
    PIP_VERSION=$(pip3 --version | awk '{print $2}')
    echo -e "${GREEN}Found${NC} (version $PIP_VERSION)"
    echo "Upgrading pip..."
    pip3 install --upgrade pip --break-system-packages 2>/dev/null || pip3 install --upgrade pip
else
    echo -e "${RED}Not found${NC}"
    echo "Installing pip..."

    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command_exists apt-get; then
            sudo apt-get update
            sudo apt-get install -y python3-pip
        elif command_exists yum; then
            sudo yum install -y python3-pip
        elif command_exists dnf; then
            sudo dnf install -y python3-pip
        else
            echo -e "${RED}Unable to install pip. Please install manually.${NC}"
            exit 1
        fi
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        if command_exists brew; then
            brew install python3
        else
            echo -e "${RED}Homebrew not found. Please install pip manually.${NC}"
            exit 1
        fi
    else
        echo -e "${RED}Unable to install pip. Please install manually.${NC}"
        exit 1
    fi
fi

# Check uv package manager
echo -n "Checking uv package manager... "
if command_exists uv; then
    UV_VERSION=$(uv --version | awk '{print $2}')
    echo -e "${GREEN}Found${NC} (version $UV_VERSION)"
else
    echo -e "${YELLOW}Not found${NC}"
    echo "Installing uv..."
    pip3 install uv --break-system-packages 2>/dev/null || pip3 install uv
fi

# Check wget
echo -n "Checking wget... "
if command_exists wget; then
    echo -e "${GREEN}Found${NC}"
else
    echo -e "${YELLOW}Not found${NC}"
    echo "Installing wget..."

    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command_exists apt-get; then
            sudo apt-get update
            sudo apt-get install -y wget
        elif command_exists yum; then
            sudo yum install -y wget
        elif command_exists dnf; then
            sudo dnf install -y wget
        else
            echo -e "${RED}Unable to install wget. Please install manually.${NC}"
            exit 1
        fi
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        if command_exists brew; then
            brew install wget
        else
            echo -e "${RED}Homebrew not found. Please install wget manually.${NC}"
            exit 1
        fi
    else
        echo -e "${RED}Unsupported OS. Please install wget manually.${NC}"
        exit 1
    fi
fi

# Check GitHub CLI
echo -n "Checking GitHub CLI (gh)... "
if command_exists gh; then
    GH_VERSION=$(gh --version | head -n 1 | awk '{print $3}')
    echo -e "${GREEN}Found${NC} (version $GH_VERSION)"
else
    echo -e "${YELLOW}Not found${NC}"
    echo "Installing GitHub CLI..."

    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command_exists apt-get; then
            sudo mkdir -p -m 755 /etc/apt/keyrings
            wget -nv -O /tmp/githubcli-archive-keyring.gpg https://cli.github.com/packages/githubcli-archive-keyring.gpg
            sudo cat /tmp/githubcli-archive-keyring.gpg | sudo tee /etc/apt/keyrings/githubcli-archive-keyring.gpg > /dev/null
            sudo chmod go+r /etc/apt/keyrings/githubcli-archive-keyring.gpg
            echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | sudo tee /etc/apt/sources.list.d/github-cli.list > /dev/null
            sudo apt-get update
            sudo apt-get install -y gh
            rm -f /tmp/githubcli-archive-keyring.gpg
        elif command_exists yum; then
            sudo yum install -y 'dnf-command(config-manager)'
            sudo yum config-manager --add-repo https://cli.github.com/packages/rpm/gh-cli.repo
            sudo yum install -y gh
        elif command_exists dnf; then
            sudo dnf install -y 'dnf-command(config-manager)'
            sudo dnf config-manager --add-repo https://cli.github.com/packages/rpm/gh-cli.repo
            sudo dnf install -y gh
        else
            echo -e "${RED}Unable to install GitHub CLI. Please install manually.${NC}"
            echo "Visit: https://cli.github.com/manual/installation"
        fi
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        if command_exists brew; then
            brew install gh
        else
            echo -e "${RED}Homebrew not found. Please install GitHub CLI manually.${NC}"
            echo "Visit: https://cli.github.com/manual/installation"
        fi
    else
        echo -e "${RED}Unsupported OS. Please install GitHub CLI manually.${NC}"
        echo "Visit: https://cli.github.com/manual/installation"
    fi
fi

# Sync dependencies with uv
echo ""
echo "Syncing project dependencies..."

# Change to backend directory if it exists
if [ -d "backend" ]; then
    cd backend
    echo "Changed to backend directory"
fi

if [ -f "pyproject.toml" ] && [ -f "uv.lock" ]; then
    uv sync
else
    echo -e "${YELLOW}Warning: pyproject.toml or uv.lock not found. Skipping uv sync.${NC}"
fi

# Install Python packages
echo ""
echo "Installing Python packages..."

PACKAGES=("ruff" "langgraph-cli" "langchain-experimental" "fastapi")

for package in "${PACKAGES[@]}"; do
    echo -n "Checking $package... "
    if python3 -c "import ${package//-/_}" 2>/dev/null || command_exists "$package"; then
        echo -e "${GREEN}Found${NC}"
    else
        echo -e "${YELLOW}Not found${NC}"
        echo "Installing $package..."
        if [ "$package" == "langgraph-cli" ]; then
            uv pip install --system --upgrade langgraph-cli 'langgraph-cli[inmem]'
        else
            uv pip install --system --upgrade "$package"
        fi
    fi
done

echo ""
echo -e "${GREEN}==================================="
echo "Setup completed successfully!"
echo "===================================${NC}"
echo ""
echo "Note: If you want to install the package in editable mode, run:"
echo "  pip install -e ."
echo ""
