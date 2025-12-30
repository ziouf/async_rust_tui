#!/usr/bin/env bash
set -e

# Exit early if running in GitHub Codespaces
if [[ ${CODESPACES:-false} == true ]]; then
    echo "Skipping slides server autostart in Codespaces"
    exit 0
fi

# Start the slides server on a random available port
nohup bash -c 'PORT=0 ./server.py &'
