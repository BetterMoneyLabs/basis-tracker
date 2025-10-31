# Basis Tracker Server Management Scripts

These scripts help manage the Basis Tracker server in the background with proper logging.

## Available Scripts

### `run_server.sh`
Starts the Basis Tracker server in the background using `nohup` and logs output to `server.log`.

**Usage:**
```bash
./run_server.sh
```

**Features:**
- Builds the server if not already built
- Checks for existing running instances
- Starts server with `nohup` for background execution
- Logs all output to `server.log`
- Creates `server.pid` file with process ID

### `stop_server.sh`
Gracefully stops the running Basis Tracker server.

**Usage:**
```bash
./stop_server.sh
```

**Features:**
- Reads PID from `server.pid` file
- Sends SIGTERM for graceful shutdown
- Falls back to SIGKILL if needed
- Removes PID file after stopping

### `server_status.sh`
Shows the current status of the Basis Tracker server.

**Usage:**
```bash
./server_status.sh
```

**Features:**
- Checks if server is running
- Shows process details (CPU, memory, uptime)
- Displays recent log entries
- Provides log file information

## Files Created

- `server.log` - Server output and error logs
- `server.pid` - Process ID file for management

## Example Workflow

1. **Start the server:**
   ```bash
   ./run_server.sh
   ```

2. **Check server status:**
   ```bash
   ./server_status.sh
   ```

3. **View live logs:**
   ```bash
   tail -f server.log
   ```

4. **Stop the server:**
   ```bash
   ./stop_server.sh
   ```

## Notes

- The server runs on `127.0.0.1:3048` by default
- Logs are written to `server.log` in the current directory
- The server binary is built automatically if not found
- All scripts include colored output for better readability
