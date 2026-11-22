#!/bin/sh
set -e

# git-lineage universal installer for Linux and macOS
REPO="NayanSrivastav/git-lineage"

print_step() {
  printf "\033[1;36m==>\033[0m \033[1m%s\033[0m\n" "$1"
}

print_error() {
  printf "\033[1;31merror:\033[0m %s\n" "$1" >&2
}

# 1. Detect OS
OS="$(uname -s)"
case "$OS" in
  Linux)  OS_TARGET="unknown-linux-musl" ;;
  Darwin) OS_TARGET="apple-darwin" ;;
  *)
    print_error "Unsupported operating system: $OS. Please install via Cargo."
    exit 1
    ;;
esac

# 2. Detect Architecture
ARCH="$(uname -m)"
case "$ARCH" in
  x86_64|amd64) ARCH_TARGET="x86_64" ;;
  arm64|aarch64) ARCH_TARGET="aarch64" ;;
  *)
    print_error "Unsupported CPU architecture: $ARCH. Please install via Cargo."
    exit 1
    ;;
esac

TARGET="${ARCH_TARGET}-${OS_TARGET}"
print_step "Detected system: ${OS} (${ARCH}) -> Target: ${TARGET}"

# 3. Determine latest release version
if [ -z "$VERSION" ]; then
  print_step "Checking latest release on GitHub..."
  VERSION=$(curl -s "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/' || echo "")
  if [ -z "$VERSION" ]; then
    VERSION="v0.2.0"
  fi
fi

print_step "Installing git-lineage ${VERSION}..."

TARBALL="git-lineage-${VERSION}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${TARBALL}"

TEMP_DIR=$(mktemp -d)
trap 'rm -rf "$TEMP_DIR"' EXIT

print_step "Downloading binary package from GitHub..."
if ! curl -fsSL "$DOWNLOAD_URL" -o "$TEMP_DIR/$TARBALL"; then
  print_error "Failed to download $TARBALL from $DOWNLOAD_URL"
  print_error "Prebuilt binaries may not yet exist for $TARGET at $VERSION."
  exit 1
fi

# 4. Extract
tar -xzf "$TEMP_DIR/$TARBALL" -C "$TEMP_DIR"

# 5. Determine installation destination
if [ -w "/usr/local/bin" ]; then
  INSTALL_DIR="/usr/local/bin"
elif [ -w "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
  INSTALL_DIR="$HOME/.local/bin"
else
  INSTALL_DIR="/usr/local/bin"
fi

print_step "Installing git-lineage binary to ${INSTALL_DIR}..."
if [ -w "$INSTALL_DIR" ]; then
  mv "$TEMP_DIR/git-lineage" "$INSTALL_DIR/git-lineage"
  chmod +x "$INSTALL_DIR/git-lineage"
else
  print_step "Requires sudo permissions to install into ${INSTALL_DIR}:"
  sudo mv "$TEMP_DIR/git-lineage" "$INSTALL_DIR/git-lineage"
  sudo chmod +x "$INSTALL_DIR/git-lineage"
fi

print_step "Verification:"
"$INSTALL_DIR/git-lineage" --version || true

printf "\n\033[1;32m✓ Successfully installed git-lineage to %s/git-lineage!\033[0m\n" "$INSTALL_DIR"
printf "Run \033[1mgit lineage --help\033[0m to get started.\n\n"
