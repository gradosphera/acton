#!/usr/bin/env bash
set -euo pipefail

# Colors
if [[ -t 1 ]]; then
  RED='\033[0;31m'
  GREEN='\033[0;32m'
  YELLOW='\033[1;33m'
  BLUE='\033[0;34m'
  BOLD='\033[1m'
  NC='\033[0m' # No Color
else
  RED=''
  GREEN=''
  YELLOW=''
  BLUE=''
  BOLD=''
  NC=''
fi

function info() {
  echo -e "${BLUE}${BOLD}==>${NC} ${BOLD}$1${NC}"
}

function success() {
  echo -e "${GREEN}${BOLD}==>${NC} ${BOLD}$1${NC}"
}

function warn() {
  echo -e "${YELLOW}${BOLD}warning:${NC} $1"
}

function error() {
  echo -e "${RED}${BOLD}error:${NC} $1" >&2
}

function check_dependency() {
  if ! command -v "$1" >/dev/null 2>&1; then
    error "Required dependency '$1' is not installed."
    exit 1
  fi
}

# Check dependencies
check_dependency "curl"
check_dependency "tar"
check_dependency "python3"

REPO="i582/acton"
BIN="acton"
DEST="${HOME}/.acton/bin"
TAG="${1:-latest}"

: "${GITHUB_TOKEN:?Set GITHUB_TOKEN (token must have access to ${REPO})}"

# OS
case "$(uname -s)" in
  Darwin) OS="darwin" ;;
  Linux)  OS="linux" ;;
  *) error "Unsupported OS: $(uname -s)"; exit 1 ;;
esac

# ARCH
case "$(uname -m)" in
  arm64|aarch64) ARCH="arm64" ;;
  x86_64|amd64)  ARCH="x86_64" ;;
  *) error "Unsupported architecture: $(uname -m)"; exit 1 ;;
esac

# Acton supports linux-x86_64 only for now
if [[ "$OS" == "linux" && "$ARCH" != "x86_64" ]]; then
  error "Unsupported platform: ${OS}-${ARCH} (linux-x86_64 only)"
  exit 1
fi

API="https://api.github.com/repos/${REPO}"
AUTH=(-H "Authorization: Bearer ${GITHUB_TOKEN}" -H "Accept: application/vnd.github+json")

info "Fetching release information for ${TAG}..."
# Fetch release JSON
if [[ "$TAG" == "latest" ]]; then
  URL="${API}/releases/latest"
else
  # Add 'v' prefix if missing for version-like tags (common in GitHub releases)
  if [[ "$TAG" =~ ^[0-9] ]]; then
    CLEAN_TAG="v$TAG"
  else
    CLEAN_TAG="$TAG"
  fi
  URL="${API}/releases/tags/${CLEAN_TAG}"
fi

release_json="$(curl -fsSL "${AUTH[@]}" "$URL" 2>/dev/null)" || {
  echo -e "\n${RED}${BOLD}error:${NC} Could not find release ${BOLD}${TAG}${NC} on GitHub." >&2
  echo -e "Check if the tag exists here: ${BLUE}https://github.com/${REPO}/releases${NC}" >&2
  exit 1
}

# Get tag_name from release JSON
tag_name="$(
  python3 -c 'import json,sys; print(json.load(sys.stdin).get("tag_name", ""))' <<<"$release_json"
)"

if [[ -z "$tag_name" ]]; then
  error "Failed to parse release information. The GitHub API response might be invalid."
  exit 1
fi

ASSET="${BIN}-${tag_name}-${OS}-${ARCH}.tar.gz"

# Find asset id by exact name
asset_id="$(
  python3 -c '
import json, sys
r = json.load(sys.stdin)
want = sys.argv[1]
print(next((str(a["id"]) for a in r.get("assets", []) if a.get("name") == want), ""))
' "$ASSET" <<<"$release_json"
)"

if [[ -z "${asset_id:-}" ]]; then
  error "Asset not found in release ${tag_name}: ${ASSET}"
  exit 1
fi

tmp="$(mktemp -d)"

mkdir -p "$DEST"

info "Downloading ${ASSET}..."
curl -fL --progress-bar \
  -H "Authorization: Bearer ${GITHUB_TOKEN}" \
  -H "Accept: application/octet-stream" \
  "${API}/releases/assets/${asset_id}" \
  -o "$tmp/$ASSET"

info "Extracting and installing to ${DEST}..."
tar -xzf "$tmp/$ASSET" -C "$tmp"
# Find the binary
BIN_PATH="$(find "$tmp" -maxdepth 3 -type f -name "$BIN" | head -n1)"

if [[ -z "$BIN_PATH" ]]; then
  error "Could not find binary '${BIN}' in the downloaded archive."
  exit 1
fi

install -m 0755 "$BIN_PATH" "$DEST/$BIN"

# Final success message and instructions
echo
success "Acton ${tag_name} has been installed successfully!"

echo
info "Verify installation by running:"
echo -e "${BOLD}$DEST/$BIN --version${NC}"
echo

# Check if DEST is in PATH
if [[ ":$PATH:" != *":$DEST:"* ]]; then
    echo -e "Add Acton to your PATH by adding this line to your shell profile (e.g., ~/.zshrc or ~/.bashrc):"
    echo -e "${BOLD}export PATH=\"$DEST:\$PATH\"${NC}"
    echo
    echo -e "Then, restart your terminal or source your profile."
else
    echo -e "${GREEN}${BOLD}Acton is already in your PATH.${NC}"
fi

echo
echo -e "We recommend enabling shell completions for a better experience"
echo -e "Learn more in the documentation: ${BLUE}https://i582.github.io/acton/docs/commands/shell-completions/${NC}"
