#!/bin/bash

# Check if a command was provided
if [ -z "$1" ]; then
    echo "Usage: $0 <command_to_run>"
    exit 1
fi

# Log file
LOGFILE="memory_usage_$(date +%s).txt"
echo "Logging to $LOGFILE"

# Start the process in the background
"$@" &  # Runs the provided command
PID=$!
echo "Started process with PID: $PID"

# Header for the log
echo "Timestamp %MEM RSS(kB) VSZ(kB)" > "$LOGFILE"

# Monitor memory usage until the process ends
while ps -p $PID > /dev/null; do
    TIMESTAMP=$(date '+%s')
    MEMORY=$(ps -p $PID -o %mem,rss,vsz --no-headers)
    if [ -n "$MEMORY" ]; then
        echo "$TIMESTAMP $MEMORY" >> "$LOGFILE"
    fi
    sleep 1  # Adjust polling interval as needed (e.g., 0.5 for 500ms)
done

echo "Process $PID has finished."

# Generate a simple report
echo "Memory Usage Report" >> "$LOGFILE"
echo "-----------------" >> "$LOGFILE"
echo "Max %MEM: $(awk 'NR>1{print $2}' "$LOGFILE" | sort -n | tail -1)" >> "$LOGFILE"
echo "Max RSS (kB): $(awk 'NR>1{print $3}' "$LOGFILE" | sort -n | tail -1)" >> "$LOGFILE"
echo "Max VSZ (kB): $(awk 'NR>1{print $4}' "$LOGFILE" | sort -n | tail -1)" >> "$LOGFILE"

echo "Report generated in $LOGFILE"