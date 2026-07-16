#!/usr/bin/env bash
# BINARY + ASSETS required. Optional: VERSION, ARCH, OUT_DIR.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PKG_DIR="${ROOT}/packaging/debian"
VERSION_SCRIPT="${ROOT}/packaging/scripts/package-version.sh"

BINARY="${BINARY:?BINARY is required (path to zoeken-server)}"
ASSETS="${ASSETS:?ASSETS is required (path to SPA assets)}"
OUT_DIR="${OUT_DIR:-${ROOT}/dist}"
VERSION="${VERSION:-$("${VERSION_SCRIPT}")}"
VERSION="${VERSION#v}"

if [[ ! -f "${BINARY}" ]]; then
  echo "BINARY not found: ${BINARY}" >&2
  exit 1
fi
if [[ ! -f "${ASSETS}/index.html" ]]; then
  echo "ASSETS must contain index.html: ${ASSETS}" >&2
  exit 1
fi

detect_arch() {
  if [[ -n "${ARCH:-}" ]]; then
    echo "${ARCH}"
    return
  fi
  if command -v dpkg >/dev/null 2>&1; then
    if command -v file >/dev/null 2>&1; then
      local info
      info="$(file -b "${BINARY}" || true)"
      case "${info}" in
        *aarch64*|*ARM\ aarch64*) echo "arm64"; return ;;
        *x86-64*|*x86_64*) echo "amd64"; return ;;
      esac
    fi
    dpkg --print-architecture
    return
  fi
  echo "amd64"
}

ARCH="$(detect_arch)"
case "${ARCH}" in
  amd64|arm64) ;;
  aarch64) ARCH="arm64" ;;
  x86_64) ARCH="amd64" ;;
  *)
    echo "unsupported ARCH=${ARCH} (want amd64 or arm64)" >&2
    exit 1
    ;;
esac

STAGE="$(mktemp -d)"
cleanup() { rm -rf "${STAGE}"; }
trap cleanup EXIT

mkdir -p \
  "${STAGE}/DEBIAN" \
  "${STAGE}/usr/bin" \
  "${STAGE}/usr/share/zoeken/assets" \
  "${STAGE}/usr/share/zoeken/plugins" \
  "${STAGE}/usr/share/doc/zoeken" \
  "${STAGE}/etc/zoeken" \
  "${STAGE}/etc/default" \
  "${STAGE}/lib/systemd/system" \
  "${STAGE}/var/lib/zoeken"

install -m 0755 "${BINARY}" "${STAGE}/usr/bin/zoeken-server"
cp -a "${ASSETS}/." "${STAGE}/usr/share/zoeken/assets/"
if [[ -d "${ROOT}/zoeken/zoeken-plugins/plugins" ]]; then
  cp -a "${ROOT}/zoeken/zoeken-plugins/plugins/." "${STAGE}/usr/share/zoeken/plugins/"
fi

install -m 0644 "${PKG_DIR}/zoeken.service" "${STAGE}/lib/systemd/system/zoeken.service"
install -m 0644 "${PKG_DIR}/zoeken.default" "${STAGE}/etc/default/zoeken"
install -m 0644 "${PKG_DIR}/zoeken.settings.yml" "${STAGE}/etc/zoeken/settings.yml"
install -m 0644 "${PKG_DIR}/limiter.toml" "${STAGE}/etc/zoeken/limiter.toml"
install -m 0644 "${PKG_DIR}/zoeken.settings.yml" "${STAGE}/usr/share/doc/zoeken/settings.yml.example"
install -m 0644 "${PKG_DIR}/limiter.toml" "${STAGE}/usr/share/doc/zoeken/limiter.toml.example"
install -m 0644 "${ROOT}/default.config.yml" "${STAGE}/usr/share/doc/zoeken/default.config.yml"
install -m 0644 "${PKG_DIR}/copyright" "${STAGE}/usr/share/doc/zoeken/copyright"
install -m 0644 "${ROOT}/LICENSE" "${STAGE}/usr/share/doc/zoeken/LICENSE"
if [[ -f "${PKG_DIR}/changelog.in" ]]; then
  sed -e "s/__VERSION__/${VERSION}/g" "${PKG_DIR}/changelog.in" \
    | gzip -n -9 > "${STAGE}/usr/share/doc/zoeken/changelog.Debian.gz"
fi

sed -e "s/__VERSION__/${VERSION}/g" -e "s/__ARCH__/${ARCH}/g" \
  "${PKG_DIR}/control.in" > "${STAGE}/DEBIAN/control"

install -m 0755 "${PKG_DIR}/postinst" "${STAGE}/DEBIAN/postinst"
install -m 0755 "${PKG_DIR}/prerm" "${STAGE}/DEBIAN/prerm"
install -m 0755 "${PKG_DIR}/postrm" "${STAGE}/DEBIAN/postrm"

cat > "${STAGE}/DEBIAN/conffiles" <<EOF
/etc/default/zoeken
/etc/zoeken/settings.yml
/etc/zoeken/limiter.toml
EOF

mkdir -p "${OUT_DIR}"
DEB="${OUT_DIR}/zoeken_${VERSION}_${ARCH}.deb"
dpkg-deb --root-owner-group --build "${STAGE}" "${DEB}"
echo "built ${DEB}"
