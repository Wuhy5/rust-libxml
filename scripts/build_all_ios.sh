#!/bin/bash
# 构建所有iOS架构的libxml2

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

# 检查是否在macOS上运行
if [[ "$(uname -s)" != "Darwin" ]]; then
    log_error "iOS builds are only supported on macOS"
    exit 1
fi

# 创建构建目录
mkdir -p "$BUILD_DIR"
mkdir -p "$PREBUILT_DIR"

log_info "Building libxml2 for iOS architectures..."

# 构建各个iOS架构
ARCHS=(arm64 x86_64)
FAILED_ARCHS=()

for arch in "${ARCHS[@]}"; do
    log_info "Building for iOS $arch..."
    if "$SCRIPT_DIR/build_libxml2.sh" \
        --platform ios \
        --arch "$arch" \
        --src-dir "$BUILD_DIR/src" \
        --install-dir "$PREBUILT_DIR/ios"; then
        log_info "✓ Successfully built for iOS $arch"
    else
        log_error "✗ Failed to build for iOS $arch"
        FAILED_ARCHS+=("$arch")
    fi
done

# 检查构建结果
if [ ${#FAILED_ARCHS[@]} -eq 0 ]; then
    log_info "All iOS architectures built successfully!"

    # 创建通用库 (Universal/Fat binary)
    log_info "Creating universal library..."
    UNIVERSAL_DIR="$PREBUILT_DIR/ios/universal"
    mkdir -p "$UNIVERSAL_DIR/lib"
    mkdir -p "$UNIVERSAL_DIR/include"

    # 复制头文件 (从任一架构复制即可)
    cp -r "$PREBUILT_DIR/ios/arm64/include/"* "$UNIVERSAL_DIR/include/"

    # 创建通用静态库
    lipo -create \
        "$PREBUILT_DIR/ios/arm64/lib/libxml2.a" \
        "$PREBUILT_DIR/ios/x86_64/lib/libxml2.a" \
        -output "$UNIVERSAL_DIR/lib/libxml2.a"

    log_info "Universal library created at: $UNIVERSAL_DIR"

    # 显示构建结果
    echo ""
    log_info "Build results:"
    for arch in "${ARCHS[@]}"; do
        lib_path="$PREBUILT_DIR/ios/$arch/lib/libxml2.a"
        if [ -f "$lib_path" ]; then
            size=$(du -h "$lib_path" | cut -f1)
            log_info "  $arch: $lib_path ($size)"
        fi
    done

    universal_lib="$UNIVERSAL_DIR/lib/libxml2.a"
    if [ -f "$universal_lib" ]; then
        size=$(du -h "$universal_lib" | cut -f1)
        log_info "  universal: $universal_lib ($size)"
    fi

    echo ""
    log_info "To use these libraries in your Rust project, set:"
    log_info "  export LIBXML2_IOS_PATH=\"$UNIVERSAL_DIR\""
else
    log_error "Build failed for architectures: ${FAILED_ARCHS[*]}"
    exit 1
fi
