#!/usr/bin/env python3
import os
import sys
import time
import json
import subprocess
import urllib.request
import urllib.error
from threading import Thread

# Configuration
PORT = 8080
BASE_URL = f"http://localhost:{PORT}"
HEALTH_URL = f"{BASE_URL}/health"
THREADS_URL = f"{BASE_URL}/threads"

def log(msg):
    print(f"[TEST] {msg}")

def check_health():
    try:
        with urllib.request.urlopen(HEALTH_URL) as response:
            return response.status == 200
    except urllib.error.URLError:
        return False

def wait_for_server(timeout=30):
    start = time.time()
    while time.time() - start < timeout:
        if check_health():
            return True
        time.sleep(1)
    return False

def create_thread():
    req = urllib.request.Request(
        THREADS_URL,
        data=json.dumps({"metadata": {"test": "true"}}).encode('utf-8'),
        headers={'Content-Type': 'application/json'}
    )
    with urllib.request.urlopen(req) as response:
        data = json.loads(response.read().decode())
        return data['thread_id']

def stream_run(thread_id, message):
    url = f"{THREADS_URL}/{thread_id}/runs/stream"
    payload = {
        "assistant_id": "test-agent",
        "input": {
            "messages": [
                {"role": "user", "content": message}
            ]
        }
    }
    
    req = urllib.request.Request(
        url,
        data=json.dumps(payload).encode('utf-8'),
        headers={'Content-Type': 'application/json'}
    )
    
    log(f"Sending message: '{message}'")
    
    try:
        with urllib.request.urlopen(req) as response:
            for line in response:
                line = line.decode().strip()
                if line.startswith("event:"):
                    event_type = line.split(":", 1)[1].strip()
                elif line.startswith("data:"):
                    data = line.split(":", 1)[1].strip()
                    print(f"[{event_type}] {data}")
    except urllib.error.HTTPError as e:
        log(f"Error streaming run: {e}")
        print(e.read().decode())

def load_env_secrets():
    secrets_path = "terminal-app/.env.secrets"
    if os.path.exists(secrets_path):
        with open(secrets_path, "r") as f:
            for line in f:
                if "=" in line and not line.startswith("#"):
                    key, value = line.strip().split("=", 1)
                    os.environ[key] = value
        log(f"Loaded secrets from {secrets_path}")

def main():
    # Load secrets first
    load_env_secrets()
    
    # check for API key
    if "ANTHROPIC_API_KEY" not in os.environ:
        log("ERROR: ANTHROPIC_API_KEY env var is missing!")
        log("Please ensure it is set in terminal-app/.env.secrets or as an environment variable.")
        sys.exit(1)

    # Start server
    log("Starting backend server...")
    
    # Check if cargo is available
    try:
        subprocess.run(["cargo", "--version"], check=True, capture_output=True)
    except FileNotFoundError:
        log("Error: cargo not found")
        sys.exit(1)

    env = os.environ.copy()
    env["ENGINE_TYPE"] = "rig"
    env["RUST_LOG"] = "info,infraware_backend=debug"
    env["PORT"] = str(PORT)

    server_process = subprocess.Popen(
        ["cargo", "run", "-p", "infraware-backend", "--features", "rig"],
        env=env,
        stdout=sys.stdout,
        stderr=sys.stderr,
        cwd=os.getcwd() 
    )

    try:
        log("Waiting for server to be healthy...")
        if wait_for_server():
            log("Server is up!")
            
            # Create thread
            thread_id = create_thread()
            log(f"Created thread: {thread_id}")
            
            # Send message
            # Asking to explicitly EXECUTE the command
            stream_run(thread_id, "Please EXECUTE the command to list files in the current directory. Do not just tell me what to do, actually run it using the tool.")
            
            log("Test completed successfully.")
        else:
            log("Server failed to start in time.")
    except Exception as e:
        log(f"An error occurred: {e}")
    finally:
        log("Stopping server...")
        server_process.terminate()
        server_process.wait()

if __name__ == "__main__":
    main()
