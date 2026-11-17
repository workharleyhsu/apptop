#!/bin/sh
# apptop installer.
#
#   curl -fsSL https://raw.githubusercontent.com/workharleyhsu/apptop/main/install.sh | sh
#
# Downloads the latest prebuilt static binary, verifies its SHA-256 checksum and
# installs it to ~/.local/bin (override with APPTOP_INSTALL_DIR). No root needed.
#
# Environment:
#   APPTOP_INSTALL_DIR   where to put the binary (default: ~/.local/bin)
#   APPTOP_VERSION       release tag to install (default: the latest release)
#   APPTOP_BASE_URL      download location override (used by the tests)

set -eu

REPO="workharleyhsu/apptop"

say() { printf '%s\n' "$*"; }
fail() { printf 'apptop install: %s\n' "$*" >&2; exit 1; }

fetch() {
    # fetch URL DESTINATION
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL --proto '=https,file' "$1" -o "$2"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$1" -O "$2"
    else
        fail "curl or wget is required"
    fi
}

main() {
    [ "$(uname -s)" = "Linux" ] || fail "only Linux is supported"
    case "$(uname -m)" in
        x86_64 | amd64) ;;
        *) fail "only x86-64 is supported for now (this machine is $(uname -m)); build from source instead: https://github.com/$REPO" ;;
    esac
    command -v tar >/dev/null 2>&1 || fail "tar is required"
    command -v sha256sum >/dev/null 2>&1 || fail "sha256sum is required"
    command -v curl >/dev/null 2>&1 || command -v wget >/dev/null 2>&1 || fail "curl or wget is required"

    # Each release also ships the tarball under a name without the version,
    # so "latest" can be downloaded directly: no API call, no rate limit.
    name="apptop-x86_64-linux"
    if [ -n "${APPTOP_BASE_URL:-}" ]; then
        base="$APPTOP_BASE_URL"
    elif [ -n "${APPTOP_VERSION:-}" ]; then
        base="https://github.com/$REPO/releases/download/$APPTOP_VERSION"
    else
        base="https://github.com/$REPO/releases/latest/download"
    fi

    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT INT TERM

    say "Downloading apptop..."
    fetch "$base/$name.tar.gz" "$tmp/$name.tar.gz"
    fetch "$base/$name.tar.gz.sha256" "$tmp/$name.tar.gz.sha256"

    # Never install something whose checksum does not match.
    (cd "$tmp" && sha256sum -c "$name.tar.gz.sha256" >/dev/null 2>&1) ||
        fail "checksum mismatch: the download is corrupt or was tampered with"

    tar -xzf "$tmp/$name.tar.gz" -C "$tmp"
    # The directory inside the archive carries the version number.
    bin=""
    for candidate in "$tmp"/apptop-*/apptop; do
        [ -f "$candidate" ] && bin="$candidate"
    done
    [ -n "$bin" ] || fail "the archive does not contain the apptop binary"

    dir="${APPTOP_INSTALL_DIR:-$HOME/.local/bin}"
    mkdir -p "$dir"
    # Copy then rename, so an apptop that is running is replaced atomically.
    cp "$bin" "$dir/.apptop.new"
    chmod 755 "$dir/.apptop.new"
    mv -f "$dir/.apptop.new" "$dir/apptop"

    say "Installed $("$dir/apptop" --version) to $dir/apptop"

    case ":$PATH:" in
        *":$dir:"*) say "Run it with: apptop" ;;
        *)
            say ""
            say "$dir is not in your PATH. Add it, then open a new terminal:"
            say "  bash/zsh:  echo 'export PATH=\"$dir:\$PATH\"' >> ~/.bashrc    (or ~/.zshrc)"
            say "  fish:      fish_add_path $dir"
            say ""
            say "Until then, run it with: $dir/apptop"
            ;;
    esac
    say "To uninstall: rm $dir/apptop"
}

# Everything runs from here, so a download cut short cannot execute half a script.
main "$@"
