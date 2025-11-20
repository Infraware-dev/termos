FROM mcr.microsoft.com/devcontainers/python:2-3.12-bullseye

# Install uv package manager
RUN pip3 install uv

# Set working directory
WORKDIR /workspaces/infraware-terminal/backend

COPY pyproject.toml ./
COPY uv.lock ./

# Sync dependencies with uv
RUN uv sync

# Install GitHub CLI
RUN apt-get update && \
    apt-get install -y wget && \
    mkdir -p -m 755 /etc/apt/keyrings && \
    wget -nv -O /tmp/githubcli-archive-keyring.gpg https://cli.github.com/packages/githubcli-archive-keyring.gpg && \
    cat /tmp/githubcli-archive-keyring.gpg | tee /etc/apt/keyrings/githubcli-archive-keyring.gpg > /dev/null && \
    chmod go+r /etc/apt/keyrings/githubcli-archive-keyring.gpg && \
    echo "deb [arch=$(dpkg --print-architecture) signed-by=/etc/apt/keyrings/githubcli-archive-keyring.gpg] https://cli.github.com/packages stable main" | tee /etc/apt/sources.list.d/github-cli.list > /dev/null && \
    apt-get update && \
    apt-get install -y gh && \
    rm -rf /var/lib/apt/lists/* /tmp/githubcli-archive-keyring.gpg

# Install Python packages (environment dependecies)
RUN uv pip install --system --upgrade \
    ruff \
    langgraph-cli \
    'langgraph-cli[inmem]'

# Install package in editable mode (this will run after workspace is mounted)
# Note: pip install -e . needs to run after the full codebase is available
