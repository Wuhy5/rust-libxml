#!/bin/bash
# 构建所有Android架构的libxml2

set -e

# 颜色输出
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_DIR="$PROJECT_ROOT/build"
PREBUILT_DIR="$PROJECT_ROOT/prebuilt"

# 默认API级别
API_LEVEL="${API_LEVEL:-21}"

# 创建构建目录
mkdir -p "$BUILD_DIR"
mkdir -p "$PREBUILT_DIR"

# 检查Android NDK
if [ -z "$ANDROID_NDK_ROOT" ]; then
    log_error "ANDROID_NDK_ROOT environment variable is not set"
    log_error "Please set it to your Android NDK installation path"
    exit 1
fi

if [ ! -d "$ANDROID_NDK_ROOT" ]; then
    log_error "Android NDK not found at: $ANDROID_NDK_ROOT"
    exit 1
fi

log_info "Building libxml2 for Android architectures..."
log_info "NDK Path: $ANDROID_NDK_ROOT"
log_info "API Level: $API_LEVEL"

# 构建各个Android架构
ARCHS=(arm64-v8a armeabi-v7a x86 x86_64)
FAILED_ARCHS=()

for arch in "${ARCHS[@]}"; do
    log_info "Building for $arch..."
    if "$SCRIPT_DIR/build_libxml2.sh" \
        --platform android \
        --arch "$arch" \
        --ndk-path "$ANDROID_NDK_ROOT" \
        --api-level "$API_LEVEL" \
        --src-dir "$BUILD_DIR/src" \
        --install-dir "$PREBUILT_DIR/android-$API_LEVEL"; then
        log_info "✓ Successfully built for $arch"
    else
        log_error "✗ Failed to build for $arch"
        FAILED_ARCHS+=("$arch")
    fi
done

# 检查构建结果
if [ ${#FAILED_ARCHS[@]} -eq 0 ]; then
    log_info "All Android architectures built successfully!"
    log_info "Prebuilt libraries are in: $PREBUILT_DIR/android-$API_LEVEL"

    # 显示构建结果
    echo ""
    log_info "Build results:"
    for arch in "${ARCHS[@]}"; do
        lib_path="$PREBUILT_DIR/android-$API_LEVEL/$arch/lib/libxml2.a"
        if [ -f "$lib_path" ]; then
            size=$(du -h "$lib_path" | cut -f1)
            log_info "  $arch: $lib_path ($size)"
        fi
    done

    echo ""
    log_info "To use these libraries in your Rust project, set:"
    log_info "  export LIBXML2_PREBUILT_PATH=\"$PREBUILT_DIR/android-$API_LEVEL\""
else
    log_error "Build failed for architectures: ${FAILED_ARCHS[*]}"
    exit 1
fi
echo "To use these libraries in your Rust project, set:"
echo "export LIBXML2_PREBUILT_PATH=$PREBUILT_DIR/android-21"
echo ""
echo "Or add to your .cargo/config.toml:"
echo "[env]"
echo "LIBXML2_PREBUILT_PATH = \"$PREBUILT_DIR/android-21\""
