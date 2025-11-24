# FastAPI Wrapper Setup Guide

This guide explains how to use the FastAPI wrapper for the LanGraph backend with authentication.

## Architecture

The FastAPI application wraps your existing LanGraph backend and provides:

1. **Authentication endpoints** (`/api/auth`, `/api/get-auth`) for managing Claude API keys
2. **Reverse proxy** for all LanGraph endpoints (runs, threads, etc.)
3. **Single-user authentication** with persistent API key storage in `.env`

## Setup

### 1. Install Dependencies

```bash
# Install or sync dependencies
uv sync
```

### 2. Start the Servers

You need to run **TWO** servers:

#### Terminal 1: Start LanGraph Server
```bash
langgraph dev
```
This starts the LanGraph server on `http://localhost:2024`

#### Terminal 2: Start FastAPI Server
```bash
python main.py
```
This starts the FastAPI server on `http://localhost:8000`

The FastAPI server acts as a gateway that:
- Handles authentication
- Proxies requests to the LanGraph server

## API Usage

### Base URL
```
http://localhost:8000
```

### Authentication Flow

#### 1. Set API Key
```bash
POST /api/auth
Content-Type: application/json

{
  "api_key": "sk-ant-your-api-key-here"
}
```

**Response:**
```json
{
  "success": true,
  "message": "API key validated and stored successfully"
}
```

This endpoint:
- Validates the API key by making a test request to Claude
- Stores the key in the `.env` file
- Returns an error if the key is invalid

#### 2. Check Authentication Status
```bash
GET /api/get-auth
```

**Response:**
```json
{
  "authenticated": true,
  "has_api_key": true
}
```

### LanGraph Endpoints (Proxied)

All LanGraph endpoints are available through FastAPI. They require authentication (API key must be set via `/api/auth`).

#### Stream a Run
```bash
POST /runs/stream
Content-Type: application/json

{
  "assistant_id": "supervisor",
  "input": {
    "messages": [
      {
        "role": "user",
        "content": "What's my AWS IP address?"
      }
    ]
  },
  "stream_mode": "messages"
}
```

#### Invoke a Run (Synchronous)
```bash
POST /runs/invoke
Content-Type: application/json

{
  "assistant_id": "supervisor",
  "input": {
    "messages": [
      {
        "role": "user",
        "content": "List files in /tmp"
      }
    ]
  }
}
```

#### Create a Thread
```bash
POST /threads
Content-Type: application/json

{
  "metadata": {
    "user_id": "user123"
  }
}
```

#### Get Thread
```bash
GET /threads/{thread_id}
```

#### Get Thread History
```bash
GET /threads/{thread_id}/history
```

#### Get Run Status
```bash
GET /runs/{run_id}
```

### Other Endpoints

#### Health Check
```bash
GET /health
```

**Response:**
```json
{
  "status": "healthy"
}
```

#### API Info
```bash
GET /
```

**Response:**
```json
{
  "name": "Infraware Terminal API",
  "version": "0.1.0",
  "endpoints": {
    "auth": "/api/auth",
    "get_auth": "/api/get-auth",
    "langgraph": "/* (proxied to LanGraph server)"
  }
}
```

## Error Handling

### 401 Unauthorized
```json
{
  "detail": "Not authenticated. Please provide an API key via /api/auth"
}
```
**Solution**: Call `/api/auth` with a valid API key first.

### 503 Service Unavailable
```json
{
  "detail": "LanGraph server is not running. Please start it with 'langgraph dev'"
}
```
**Solution**: Make sure the LanGraph server is running on `http://localhost:2024`.

### 400 Bad Request (Invalid API Key)
```json
{
  "detail": "Invalid API key format. Key should start with 'sk-ant-'"
}
```
**Solution**: Provide a valid Anthropic API key.

## Configuration

### Environment Variables

The API key is stored in `.env`:
```
ANTHROPIC_API_KEY=sk-ant-your-api-key-here
```

You can also manually edit this file, but using `/api/auth` is recommended as it validates the key.

### CORS

The FastAPI application allows all origins by default. To restrict CORS, edit `src/api/main.py`:

```python
app.add_middleware(
    CORSMiddleware,
    allow_origins=["http://localhost:3000"],  # Your frontend URL
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)
```

### Server Configuration

To change the FastAPI server port or host, edit `main.py`:

```python
uvicorn.run(
    "src.api.main:app",
    host="0.0.0.0",      # Change host
    port=8000,            # Change port
    reload=True,
    log_level="info",
)
```

To change the LanGraph server URL, edit `src/api/routes/langgraph_routes.py`:

```python
LANGGRAPH_SERVER_URL = "http://localhost:2024"  # Change this
```

## Development

### Project Structure
```
backend/
├── src/
│   ├── agents/              # LanGraph agents (unchanged)
│   └── api/                 # FastAPI wrapper
│       ├── main.py          # FastAPI app
│       ├── config.py        # Configuration management
│       ├── auth.py          # API key validation
│       ├── models.py        # Pydantic models
│       └── routes/
│           ├── auth_routes.py      # Auth endpoints
│           └── langgraph_routes.py # Proxy endpoints
├── main.py                  # Server entry point
└── pyproject.toml          # Dependencies
```

### Testing Authentication

```bash
# Set API key
curl -X POST http://localhost:8000/api/auth \
  -H "Content-Type: application/json" \
  -d '{"api_key": "sk-ant-your-key"}'

# Check status
curl http://localhost:8000/api/get-auth

# Test LanGraph proxy
curl -X POST http://localhost:8000/runs/stream \
  -H "Content-Type: application/json" \
  -d '{
    "assistant_id": "supervisor",
    "input": {
      "messages": [{"role": "user", "content": "Hello"}]
    }
  }'
```

## Troubleshooting

### "LanGraph server is not running"
Make sure you've started the LanGraph server:
```bash
langgraph dev
```

### "Not authenticated"
Set your API key first:
```bash
curl -X POST http://localhost:8000/api/auth \
  -H "Content-Type: application/json" \
  -d '{"api_key": "sk-ant-your-key"}'
```

### Port already in use
If port 8000 is already in use, change it in `main.py` or kill the process using it:
```bash
# Windows
netstat -ano | findstr :8000
taskkill /PID <pid> /F

# Linux/Mac
lsof -ti:8000 | xargs kill -9
```

## Next Steps

- Add rate limiting for the `/api/auth` endpoint
- Implement request/response logging
- Add API key rotation support
- Create client SDK for easier integration
- Add WebSocket support for better streaming
