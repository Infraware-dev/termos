#!/bin/bash

# Create logs directory if it doesn't exist
mkdir -p logs

# Start LangGraph dev server in background with logging
langgraph dev --no-browser 2>&1 | sed 's/\x1b\[[0-9;]*m//g' > logs/langgraph_server.log &
LANGGRAPH_PID=$!

echo "LangGraph server started (PID: $LANGGRAPH_PID) -> logs/langgraph_server.log"
echo "Waiting for LangGraph server to be ready..."

# Health check loop
MAX_RETRIES=30
RETRY_COUNT=0
HEALTH_CHECK_URL="http://127.0.0.1:2024/ok?check_db=0"

while [ $RETRY_COUNT -lt $MAX_RETRIES ]; do
    if curl -f -s "$HEALTH_CHECK_URL" > /dev/null 2>&1; then
        echo "LangGraph server is ready!"
        break
    fi
    RETRY_COUNT=$((RETRY_COUNT + 1))
    echo "Waiting for LangGraph server... (attempt $RETRY_COUNT/$MAX_RETRIES)"
    sleep 1
done

if [ $RETRY_COUNT -eq $MAX_RETRIES ]; then
    echo "Error: LangGraph server failed to start within $MAX_RETRIES seconds"
    kill $LANGGRAPH_PID 2>/dev/null
    exit 1
fi

echo "Starting FastAPI server in foreground..."
echo ""

# Start FastAPI server in foreground
uv run main.py
