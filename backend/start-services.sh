#!/bin/bash

# Create logs directory if it doesn't exist
mkdir -p logs

# Start LangGraph dev server in background with logging
langgraph dev --no-browser 2>&1 | sed 's/\x1b\[[0-9;]*m//g' > logs/langgraph_server.log &
LANGGRAPH_PID=$!

echo "LangGraph server started (PID: $LANGGRAPH_PID) -> logs/langgraph_server.log"
echo "Starting FastAPI server in foreground..."
echo ""

# Start FastAPI server in foreground
uv run main.py
