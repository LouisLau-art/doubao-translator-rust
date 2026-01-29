#!/usr/bin/env bash
set -euo pipefail

SERVICE_NAME=doubao-translator
INSTALL_DIR=/opt/doubao-translator-rust

if [[ ! -f .env ]]; then
  echo "Missing .env. Copy .env.example to .env and fill ARK_API_KEY." >&2
  exit 1
fi

sudo mkdir -p "$INSTALL_DIR"
sudo cp target/release/translator "$INSTALL_DIR/translator"
sudo cp .env "$INSTALL_DIR/.env"
sudo chown root:nobody "$INSTALL_DIR/.env"
sudo chmod 640 "$INSTALL_DIR/.env"
sudo chmod 755 "$INSTALL_DIR/translator"

sudo cp systemd/doubao-translator.service /etc/systemd/system/${SERVICE_NAME}.service
sudo systemctl daemon-reload
sudo systemctl enable ${SERVICE_NAME}.service
sudo systemctl restart ${SERVICE_NAME}.service

echo "Service installed and started: ${SERVICE_NAME}.service"
