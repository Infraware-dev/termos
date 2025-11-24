#!/bin/bash

# Start LangGraph dev server in background with logging
langgraph dev 2>&1 | sed 's/\x1b\[[0-9;]*m//g' > langgraph_server.log &

# Store the PID of the background process
LANGGRAPH_PID=$!
echo "LangGraph server started (PID: $LANGGRAPH_PID)"

# Start FastAPI server in foreground
echo "Starting FastAPI server..."
uv run main.py
