#!/bin/bash
# 构建libxml2的交叉编译脚本
# 仅支持Android平台

set -e

# 默认配置
LIBXML2_VERSION="2.10.3"
LIBXML2_URL="https://download.gnome.org/sources/libxml2/2.10/libxml2-${LIBXML2_VERSION}.tar.xz"
API_LEVEL="21"

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

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  -p, --platform PLATFORM    Target platform (android)"
    echo "  -a, --arch ARCH            Target architecture"
    echo "  -n, --ndk-path PATH        Path to Android NDK (required for Android)"
    echo "  -l, --api-level LEVEL      Android API level (default: 21)"
    echo "  -s, --src-dir DIR          Source directory (default: ./build/src)"
    echo "  -i, --install-dir DIR      Installation directory (default: ./build/install)"
    echo "  -h, --help                 Show this help message"
    echo ""
    echo "Android architectures: arm64-v8a, armeabi-v7a, x86, x86_64"
    echo ""
    echo "Examples:"
    echo "  $0 -p android -a arm64-v8a -n /path/to/ndk"
}

# 解析命令行参数
while [[ $# -gt 0 ]]; do
    case $1 in
    -p | --platform)
        PLATFORM="$2"
        shift 2
        ;;
    -a | --arch)
        ARCH="$2"
        shift 2
        ;;
    -n | --ndk-path)
        NDK_PATH="$2"
        shift 2
        ;;
    -l | --api-level)
        API_LEVEL="$2"
        shift 2
        ;;
    -s | --src-dir)
        SRC_DIR="$2"
        shift 2
        ;;
    -i | --install-dir)
        INSTALL_DIR="$2"
        shift 2
        ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        log_error "Unknown option: $1"
        usage
        exit 1
        ;;
    esac
done

# 检查必需参数
if [[ -z "$PLATFORM" || -z "$ARCH" ]]; then
    log_error "Platform and architecture are required"
    usage
    exit 1
fi

# 设置默认目录
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
SRC_DIR="${SRC_DIR:-$PROJECT_ROOT/build/src}"
INSTALL_DIR="${INSTALL_DIR:-$PROJECT_ROOT/build/install}"

# 创建目录
mkdir -p "$SRC_DIR"
mkdir -p "$INSTALL_DIR"

# Android架构映射函数
get_android_toolchain_prefix() {
    local arch="$1"
    case "$arch" in
    "arm64-v8a") echo "aarch64-linux-android" ;;
    "armeabi-v7a") echo "armv7a-linux-androideabi" ;;
    "x86") echo "i686-linux-android" ;;
    "x86_64") echo "x86_64-linux-android" ;;
    *) echo "" ;;
    esac
}

download_libxml2() {
    local src_dir="$1"
    local tarball="$src_dir/libxml2-${LIBXML2_VERSION}.tar.xz"
    local extracted_dir="$src_dir/libxml2-${LIBXML2_VERSION}"
    local target_dir="$src_dir/libxml2"

    if [[ -d "$target_dir" ]]; then
        log_info "libxml2 source already exists, skipping download"
        return 0
    fi

    log_info "Downloading libxml2 source..."
    if ! curl -L -o "$tarball" "$LIBXML2_URL"; then
        log_error "Failed to download libxml2 source"
        return 1
    fi

    log_info "Extracting libxml2 source..."
    if ! tar -xf "$tarball" -C "$src_dir"; then
        log_error "Failed to extract libxml2 source"
        return 1
    fi

    # 重命名目录
    if [[ -d "$extracted_dir" && ! -d "$target_dir" ]]; then
        mv "$extracted_dir" "$target_dir"
    fi

    log_info "libxml2 source prepared"
    return 0
}

build_android() {
    local arch="$1"
    local ndk_path="$2"
    local api_level="$3"
    local src_dir="$4"
    local install_dir="$5" # 检查架构支持
    local toolchain_prefix=$(get_android_toolchain_prefix "$arch")
    if [[ -z "$toolchain_prefix" ]]; then
        log_error "Unsupported Android architecture: $arch"
        return 1
    fi

    # 检查NDK
    if [[ ! -d "$ndk_path" ]]; then
        log_error "Android NDK not found at: $ndk_path"
        return 1
    fi

    # 确定主机标签
    local host_tag
    case "$(uname -s)" in
    Darwin*) host_tag="darwin-x86_64" ;;
    Linux*) host_tag="linux-x86_64" ;;
    CYGWIN* | MINGW* | MSYS*) host_tag="windows-x86_64" ;;
    *)
        log_error "Unsupported host OS: $(uname -s)"
        return 1
        ;;
    esac

    local toolchain_bin="$ndk_path/toolchains/llvm/prebuilt/$host_tag/bin"

    if [[ ! -d "$toolchain_bin" ]]; then
        log_error "Toolchain not found at: $toolchain_bin"
        return 1
    fi

    # 设置环境变量
    export AR="$toolchain_bin/llvm-ar"
    export CC="$toolchain_bin/${toolchain_prefix}${api_level}-clang"
    export CXX="$toolchain_bin/${toolchain_prefix}${api_level}-clang++"
    export RANLIB="$toolchain_bin/llvm-ranlib"
    export STRIP="$toolchain_bin/llvm-strip"
    export LIBS="-ldl"

    # 确保libtool使用正确的工具链
    export LDFLAGS=""
    export CPPFLAGS=""

    # 检查工具链
    if [[ ! -x "$CC" ]]; then
        log_error "Compiler not found: $CC"
        return 1
    fi

    local libxml2_src="$src_dir/libxml2"
    local arch_install_dir="$install_dir/$arch"

    # 确保安装目录存在
    mkdir -p "$arch_install_dir"

    # 获取绝对路径
    arch_install_dir="$(cd "$arch_install_dir" && pwd)"

    # 进入源码目录
    cd "$libxml2_src"

    # 清理之前的构建
    if [[ -f "Makefile" ]]; then
        make distclean || true
    fi

    log_info "Configuring libxml2 for Android $arch..."

    # 配置构建
    if ! ./autogen.sh \
        --host="$toolchain_prefix" \
        --prefix="$arch_install_dir" \
        --with-pic \
        --disable-shared \
        --without-iconv \
        --without-python \
        --without-zlib \
        --without-lzma \
        --without-http \
        --without-ftp \
        --without-debug \
        --without-catalog \
        AR="$AR" \
        CC="$CC" \
        CXX="$CXX" \
        RANLIB="$RANLIB" \
        STRIP="$STRIP" \
        LDFLAGS="$LDFLAGS" \
        CPPFLAGS="$CPPFLAGS" \
        LIBS="$LIBS"; then
        log_error "Configuration failed for $arch"
        return 1
    fi

    # 修复libtool配置
    if [[ -f "libtool" ]]; then
        log_info "Fixing libtool configuration..."
        sed -i "s|${toolchain_prefix}-ar|$AR|g" libtool
        sed -i "s|${toolchain_prefix}-ranlib|$RANLIB|g" libtool
        sed -i "s|${toolchain_prefix}-strip|$STRIP|g" libtool
    fi

    log_info "Building libxml2 for Android $arch..."

    # 编译
    if ! make -j$(nproc 2>/dev/null || echo 4); then
        log_error "Build failed for $arch"
        return 1
    fi

    log_info "Installing libxml2 for Android $arch..."

    # 安装
    if ! make install; then
        log_error "Installation failed for $arch"
        return 1
    fi

    log_info "Successfully built libxml2 for Android $arch"
    return 0
}

# 主函数
main() {
    log_info "Starting libxml2 cross-compilation for $PLATFORM $ARCH"

    # 下载libxml2源码
    if ! download_libxml2 "$SRC_DIR"; then
        log_error "Failed to download libxml2 source"
        exit 1
    fi

    if [[ -z "$NDK_PATH" ]]; then
        log_error "Android NDK path is required for Android builds"
        exit 1
    fi

    if ! build_android "$ARCH" "$NDK_PATH" "$API_LEVEL" "$SRC_DIR" "$INSTALL_DIR"; then
        log_error "Failed to build libxml2 for Android $ARCH"
        exit 1
    fi
    log_info "Build completed successfully!"
    log_info "Installation directory: $INSTALL_DIR/$ARCH"
}

# 运行主函数
main "$@"
