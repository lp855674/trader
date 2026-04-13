import subprocess
import sys

# Start lstm-service in background
proc = subprocess.Popen(
    [sys.executable, "-m", "uvicorn", "main:app", "--host", "127.0.0.1", "--port", "8000"],
    cwd="services/lstm-service",
    stdout=subprocess.DEVNULL,
    stderr=subprocess.DEVNULL,
)
print(f"Started lstm-service with PID: {proc.pid}")
