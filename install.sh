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
echo "This script will install all dependencies inside the backend virtual environment using uv."
echo ""

# Function to check if a command exists
command_exists() {
    command -v "$1" >/dev/null 2>&1
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

# Check uv package manager
echo ""
echo -n "Checking uv package manager... "
if command_exists uv; then
    UV_VERSION=$(uv --version | awk '{print $2}')
    echo -e "${GREEN}Found${NC} (version $UV_VERSION)"
else
    echo -e "${YELLOW}Not found${NC}"
    echo "Installing uv..."

    # Install uv using the official installer
    if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]]; then
        powershell -c "irm https://astral.sh/uv/install.ps1 | iex"
    else
        curl -LsSf https://astral.sh/uv/install.sh | sh
    fi

    # Refresh PATH to find newly installed uv
    export PATH="$HOME/.cargo/bin:$PATH"
    hash -r 2>/dev/null || true

    if ! command_exists uv; then
        echo -e "${RED}Failed to install uv. Please install manually.${NC}"
        echo "Visit: https://docs.astral.sh/uv/getting-started/installation/"
        exit 1
    fi
fi

# Check wget
echo ""
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

# Navigate to backend directory
echo ""
if [ ! -d "backend" ]; then
    echo -e "${RED}Error: backend directory not found${NC}"
    exit 1
fi

cd backend
echo "Changed to backend directory"

# Sync dependencies with uv (creates venv automatically)
echo ""
echo "Syncing project dependencies with uv..."
echo "This will create a virtual environment in backend/.venv"

if [ -f "pyproject.toml" ]; then
    uv sync
    echo -e "${GREEN}Dependencies synced successfully${NC}"
else
    echo -e "${YELLOW}Warning: pyproject.toml not found.${NC}"
    echo "Creating virtual environment..."
    uv venv
fi

# Install additional packages using uv
echo ""
echo "Installing additional Python packages..."

PACKAGES=("ruff" "langgraph-cli" "langchain-experimental" "fastapi")

for package in "${PACKAGES[@]}"; do
    echo "Installing $package..."
    if [ "$package" == "langgraph-cli" ]; then
        uv pip install --upgrade langgraph-cli 'langgraph-cli[inmem]'
    else
        uv pip install --upgrade "$package"
    fi
done

echo ""
echo -e "${GREEN}==================================="
echo "Setup completed successfully!"
echo "===================================${NC}"
echo ""
echo "Virtual environment location: backend/.venv"
echo ""
echo "To activate the virtual environment, run:"
if [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]]; then
    echo "  source backend/.venv/Scripts/activate"
else
    echo "  source backend/.venv/bin/activate"
fi
echo ""
echo "Or use uv to run commands directly:"
echo "  uv run python your_script.py"
echo ""
