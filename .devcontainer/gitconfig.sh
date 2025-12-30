#!/bin/bash
set -eu

# Skip if running in Codespaces
if [[ ${CODESPACES:-false} == true ]]; then
  echo "Skipping gitconfig.sh in Codespaces"
  exit 0
fi

BASEDIR=$(dirname "$0")

function init {
  git config -l --includes > "$BASEDIR/.gitconfig"
}

function postAttach {
  # Re-configures git global and local configuration with includes
  # https://github.com/microsoft/vscode-remote-release/issues/2084
  for conf in "$BASEDIR/.gitconfig"; do
    if [ -f $conf ]; then
      echo "*** Parsing Git configuration export"
      while IFS='=' read -r key value; do
        case "$key" in
        user.name | user.email | user.signingkey | commit.gpgsign)
          echo "Set Git config ${key}=${value}"
          git config --global "$key" "$value"
          ;;
        esac
      done <"$conf"
      rm -f "$conf"
    fi
  done
}

case ${1:-unknown} in 
  init)
    init
    ;;
  postAttach)
    postAttach
    ;;
  *)
    echo "Unknown command: ${1}"
    exit 1
    ;;
esac