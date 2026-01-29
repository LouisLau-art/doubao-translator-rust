#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME=doubao-translator
INSTALL_DIR=/opt/doubao-translator-rust

sudo systemctl stop ${SERVICE_NAME}.service 2>/dev/null || true
sudo systemctl disable ${SERVICE_NAME}.service 2>/dev/null || true
sudo rm -f /etc/systemd/system/${SERVICE_NAME}.service
sudo systemctl daemon-reload
sudo rm -rf "$INSTALL_DIR"

echo "Service removed: ${SERVICE_NAME}.service"
