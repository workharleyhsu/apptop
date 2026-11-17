#!/usr/bin/env bash
# Arma en dist/ todo lo que se publica en un release:
#
#   apptop-<ver>-x86_64-linux.tar.gz (+ .sha256)   binario estático + docs
#   apptop-x86_64-linux.tar.gz (+ .sha256)         lo mismo, con nombre fijo
#   apptop_<ver>-1_amd64.deb                        Debian, Ubuntu y derivadas
#   apptop-<ver>-1.x86_64.rpm                       Fedora, RHEL, openSUSE
#   SHA256SUMS                                      sumas de todo lo anterior
#
# Los tres salen del mismo binario estático (musl), así ninguno depende de la
# versión de glibc de la distribución.
#
# Requiere:
#   rustup target add x86_64-unknown-linux-musl
#   cargo install cargo-deb cargo-generate-rpm
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

rm -rf "${DIST:?}"
mkdir -p "$DIST/$NAME"

# 1. Tarball (lo usa install.sh).
cp "$BIN" README.md README.es.md LICENSE-MIT LICENSE-APACHE "$DIST/$NAME/"
tar -C "$DIST" --owner=0 --group=0 --numeric-owner -czf "$DIST/$NAME.tar.gz" "$NAME"
rm -rf "${DIST:?}/$NAME"
(cd "$DIST" && sha256sum "$NAME.tar.gz" > "$NAME.tar.gz.sha256")
# Copia con nombre fijo: install.sh la baja de releases/latest/download sin
# tener que averiguar la versión.
cp "$DIST/$NAME.tar.gz" "$DIST/apptop-x86_64-linux.tar.gz"
(cd "$DIST" && sha256sum apptop-x86_64-linux.tar.gz > apptop-x86_64-linux.tar.gz.sha256)

# 2. Paquetes. Con --target, las rutas target/release de Cargo.toml pasan a
#    apuntar al binario musl ya compilado.
cargo deb --target "$TARGET" --no-build --no-strip --output "$DIST/"
cargo generate-rpm --target "$TARGET" --output "$DIST/"

# 3. Sumas de todo.
(cd "$DIST" && sha256sum -- *.tar.gz *.deb *.rpm > SHA256SUMS)

ls -l "$DIST"
