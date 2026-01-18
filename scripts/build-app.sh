#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

# Default values
TARGET=""
RELEASE=false
OUTPUT_DIR="${PROJECT_ROOT}/target"

usage() {
    echo "Usage: $0 [--target <target>] [--release] [--output <dir>]"
    echo ""
    echo "Options:"
    echo "  --target <target>  Build target (e.g., aarch64-apple-darwin, x86_64-apple-darwin)"
    echo "  --release          Build in release mode"
    echo "  --output <dir>     Output directory (default: target/)"
    echo ""
    echo "Examples:"
    echo "  $0 --release"
    echo "  $0 --target aarch64-apple-darwin --release"
    exit 1
}

while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --release)
            RELEASE=true
            shift
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Determine build directory
if [[ -n "$TARGET" ]]; then
    if [[ "$RELEASE" == true ]]; then
        BUILD_DIR="${PROJECT_ROOT}/target/${TARGET}/release"
    else
        BUILD_DIR="${PROJECT_ROOT}/target/${TARGET}/debug"
    fi
else
    if [[ "$RELEASE" == true ]]; then
        BUILD_DIR="${PROJECT_ROOT}/target/release"
    else
        BUILD_DIR="${PROJECT_ROOT}/target/debug"
    fi
fi

# Build arguments
CARGO_ARGS=()
if [[ -n "$TARGET" ]]; then
    CARGO_ARGS+=(--target "$TARGET")
fi
if [[ "$RELEASE" == true ]]; then
    CARGO_ARGS+=(--release)
fi

echo "Building yashiki..."
cargo build -p yashiki -p yashiki-layout-tatami -p yashiki-layout-byobu "${CARGO_ARGS[@]}"

# Get version from Cargo.toml
VERSION=$(grep '^version' "${PROJECT_ROOT}/Cargo.toml" | head -1 | sed 's/.*"\(.*\)".*/\1/')
echo "Version: ${VERSION}"

# Determine architecture suffix for zip name
if [[ -n "$TARGET" ]]; then
    case "$TARGET" in
        aarch64-apple-darwin)
            ARCH_SUFFIX="-arm64"
            ;;
        x86_64-apple-darwin)
            ARCH_SUFFIX="-x86_64"
            ;;
        *)
            ARCH_SUFFIX="-${TARGET}"
            ;;
    esac
else
    # Detect current architecture
    CURRENT_ARCH=$(uname -m)
    case "$CURRENT_ARCH" in
        arm64)
            ARCH_SUFFIX="-arm64"
            ;;
        x86_64)
            ARCH_SUFFIX="-x86_64"
            ;;
        *)
            ARCH_SUFFIX=""
            ;;
    esac
fi

APP_NAME="Yashiki.app"
APP_DIR="${OUTPUT_DIR}/${APP_NAME}"

echo "Creating ${APP_NAME}..."

# Create app bundle structure
rm -rf "${APP_DIR}"
mkdir -p "${APP_DIR}/Contents/MacOS"
mkdir -p "${APP_DIR}/Contents/Resources/layouts"

# Copy binaries
cp "${BUILD_DIR}/yashiki" "${APP_DIR}/Contents/MacOS/"
cp "${BUILD_DIR}/yashiki-layout-tatami" "${APP_DIR}/Contents/Resources/layouts/"
cp "${BUILD_DIR}/yashiki-layout-byobu" "${APP_DIR}/Contents/Resources/layouts/"

# Generate Info.plist
sed "s/VERSION_PLACEHOLDER/${VERSION}/g" "${PROJECT_ROOT}/Info.plist.template" > "${APP_DIR}/Contents/Info.plist"

# Ad-hoc code signing
echo "Signing ${APP_NAME}..."
codesign --force --deep -s - "${APP_DIR}"

echo "Created: ${APP_DIR}"

# Create zip for release
if [[ "$RELEASE" == true ]]; then
    ZIP_NAME="Yashiki${ARCH_SUFFIX}-${VERSION}.zip"
    echo "Creating ${ZIP_NAME}..."
    (cd "${OUTPUT_DIR}" && zip -r "${ZIP_NAME}" "${APP_NAME}")
    echo "Created: ${OUTPUT_DIR}/${ZIP_NAME}"
fi

echo "Done!"
