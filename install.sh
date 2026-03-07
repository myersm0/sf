#!/bin/sh
set -e

REPO="myersm0/sf"
BIN_DIR="${HOME}/.local/bin"
SHARE_DIR="${HOME}/.local/share/sf"

info() { printf "\033[0;34m%s\033[0m\n" "$*"; }
err()  { printf "\033[0;31m%s\033[0m\n" "$*" >&2; exit 1; }

detect_platform() {
	os="$(uname -s)"
	arch="$(uname -m)"

	case "$os" in
		Linux)  os="linux" ;;
		Darwin) os="macos" ;;
		*)      err "Unsupported OS: $os" ;;
	esac

	case "$arch" in
		x86_64|amd64)  arch="x86_64" ;;
		arm64|aarch64) arch="aarch64" ;;
		*)             err "Unsupported architecture: $arch" ;;
	esac

	echo "sf-${os}-${arch}"
}

main() {
	artifact="$(detect_platform)"
	info "Detected platform: ${artifact}"

	url="https://github.com/${REPO}/releases/latest/download/${artifact}.tar.gz"
	info "Downloading ${url}..."

	tmpdir="$(mktemp -d)"
	trap 'rm -rf "$tmpdir"' EXIT

	if command -v curl >/dev/null 2>&1; then
		curl -fsSL "$url" -o "${tmpdir}/sf.tar.gz"
	elif command -v wget >/dev/null 2>&1; then
		wget -qO "${tmpdir}/sf.tar.gz" "$url"
	else
		err "Neither curl nor wget found."
	fi

	tar xzf "${tmpdir}/sf.tar.gz" -C "$tmpdir"

	mkdir -p "$BIN_DIR" "$SHARE_DIR"
	cp "${tmpdir}/${artifact}" "${BIN_DIR}/sf"
	chmod +x "${BIN_DIR}/sf"
	info "Installed binary to ${BIN_DIR}/sf"

	if [ -f "${tmpdir}/shell/sf.sh" ]; then
		cp "${tmpdir}/shell/sf.sh" "${SHARE_DIR}/sf.sh"
	elif [ -f "${tmpdir}/sf.sh" ]; then
		cp "${tmpdir}/sf.sh" "${SHARE_DIR}/sf.sh"
	fi
	info "Installed shell functions to ${SHARE_DIR}/sf.sh"

	echo ""
	info "Add the following to your shell profile (.bashrc, .zshrc, etc.):"
	echo ""
	case ":$PATH:" in
		*":${BIN_DIR}:"*) ;;
		*) echo "  export PATH=\"\$HOME/.local/bin:\$PATH\"" ;;
	esac
	echo "  source \"\$HOME/.local/share/sf/sf.sh\""
	echo ""
	info "Then restart your shell."
	info "Requires ollama running locally: https://ollama.com"
	info "Pull an embedding model: ollama pull qwen3-embedding"
}

main
