#!/usr/bin/env bash
# Arma el binario precompilado para publicar: estático (musl), sin
# dependencias de la distribución, empaquetado con README y suma SHA-256.
#
# Uso: scripts/package.sh
# Requiere: rustup target add x86_64-unknown-linux-musl
set -euo pipefail

cd "$(dirname "$0")/.."
TARGET=x86_64-unknown-linux-musl
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
NAME="apptop-$VERSION-x86_64-linux"
DIST=dist

cargo build --release --target "$TARGET"
BIN="target/$TARGET/release/apptop"

# El binario publicado no debe depender de bibliotecas del sistema.
if ldd "$BIN" 2>&1 | grep -q '=>'; then
    echo "error: $BIN no es estático" >&2
    exit 1
fi

rm -rf "${DIST:?}/$NAME"
mkdir -p "$DIST/$NAME"
cp "$BIN" README.md LICENSE-MIT LICENSE-APACHE "$DIST/$NAME/"

tar -C "$DIST" --owner=0 --group=0 --numeric-owner -czf "$DIST/$NAME.tar.gz" "$NAME"
(cd "$DIST" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
rm -rf "${DIST:?}/$NAME"

ls -l "$DIST/$NAME.tar.gz" "$DIST/$NAME.tar.gz.sha256"
