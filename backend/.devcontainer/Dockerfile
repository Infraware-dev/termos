FROM mcr.microsoft.com/devcontainers/python:2-3.12-bullseye

# Install uv package manager
RUN pip3 install uv

# Set working directory
WORKDIR /workspaces/infraware-terminal/backend

COPY pyproject.toml ./
COPY uv.lock ./

# Sync dependencies with uv
RUN uv sync

# Install langgraph-cli
RUN pip install --upgrade pip && pip install ruff && pip3 install langgraph-cli && pip install -U 'langgraph-cli[inmem]'

# Install package in editable mode (this will run after workspace is mounted)
# Note: pip install -e . needs to run after the full codebase is available
