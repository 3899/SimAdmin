#!/bin/bash

# 构建脚本 - 构建后端和前端，自动生成 OTA 包

set -euo pipefail

# 切换到项目根目录
cd "$(dirname "$0")/.."

TARGET="${TARGET:-aarch64-unknown-linux-musl}"

normalize_target() {
    case "$1" in
        aarch64|arm64|aarch64-unknown-linux-musl)
            echo "aarch64-unknown-linux-musl"
            ;;
        x86_64|amd64|x86_64-unknown-linux-musl)
            echo "x86_64-unknown-linux-musl"
            ;;
        *)
            echo "❌ 错误: 不支持的构建目标: $1" >&2
            echo "支持: aarch64-unknown-linux-musl, x86_64-unknown-linux-musl" >&2
            exit 1
            ;;
    esac
}

is_macos() {
    [[ "${OSTYPE:-}" == darwin* ]]
}

is_windows_bash() {
    [[ "${OSTYPE:-}" == msys* || "${OSTYPE:-}" == cygwin* || "${OSTYPE:-}" == win32* ]]
}

require_cmd() {
    local cmd="$1"
    local hint="${2:-}"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        echo "❌ 错误: 未找到命令: $cmd"
        if [ -n "$hint" ]; then
            echo "$hint"
        fi
        exit 1
    fi
}

run_sed_in_place() {
    local file="$1"
    local expression="$2"

    if is_macos; then
        sed -i '' "$expression" "$file"
    else
        sed -i "$expression" "$file"
    fi
}

print_windows_notice() {
    if is_windows_bash; then
        echo "⚠️  检测到 Windows Bash 环境。"
        echo "   完整 OTA 后端需要 Linux musl 交叉工具链 ($TARGET)。"
        echo "   推荐在 WSL2 Ubuntu 中运行: ./scripts/build.sh --no-upx"
        echo ""
    fi
}

check_windows_node_lifecycle() {
    if is_windows_bash && ! cmd.exe //c node -v >/dev/null 2>&1; then
        echo "❌ 错误: 当前 Git Bash 环境下，Node.js 无法被 npm/pnpm 生命周期脚本识别。"
        echo "请改用 WSL2 运行完整 OTA 构建，或修正 Git Bash/Windows PATH 后再执行。"
        echo "也可以在原生 PowerShell 中单独运行前端构建: pnpm build"
        exit 1
    fi
}

# 解析命令行参数
BUILD_BACKEND=true
BUILD_FRONTEND=true
USE_UPX=true  # 默认启用 UPX 压缩
SKIP_OTA=false

for arg in "$@"; do
    case $arg in
        --backend-only)
            BUILD_FRONTEND=false
            ;;
        --frontend-only)
            BUILD_BACKEND=false
            ;;
        --no-upx)
            USE_UPX=false
            ;;
        --no-ota)
            SKIP_OTA=true
            ;;
        --target=*)
            TARGET="${arg#*=}"
            ;;
        --help|-h)
            echo "用法: ./scripts/build.sh [选项]"
            echo ""
            echo "选项:"
            echo "  --backend-only   只构建后端"
            echo "  --frontend-only  只构建前端"
            echo "  --no-upx         禁用 UPX 压缩 (默认启用)"
            echo "  --no-ota         跳过 OTA 包生成"
            echo "  --target=TARGET   构建目标: aarch64 或 x86_64 (默认: aarch64)"
            echo "  --help, -h       显示帮助信息"
            echo ""
            echo "示例:"
            echo "  ./scripts/build.sh                    # 构建 + UPX + OTA 包"
            echo "  ./scripts/build.sh --no-upx           # 不压缩"
            echo "  ./scripts/build.sh --no-ota           # 不生成 OTA 包"
            echo "  ./scripts/build.sh --frontend-only    # 仅构建前端"
            echo "  ./scripts/build.sh --target=x86_64    # 构建 x86_64 OTA 包"
            exit 0
            ;;
    esac
done

TARGET="$(normalize_target "$TARGET")"

print_windows_notice

# ==================== 同步版本号 ====================
VERSION_FILE="VERSION"
if [ -f "$VERSION_FILE" ]; then
    VERSION=$(cat "$VERSION_FILE" | tr -d '[:space:]')
else
    VERSION="3.0.0"
    echo "⚠️  VERSION 文件不存在，使用默认版本: $VERSION"
fi

echo "📦 版本号: $VERSION"

require_cmd sed "请安装 sed，或在 WSL2/Git Bash 环境中运行构建脚本。"

# 更新 package.json 版本号
if [ -f "frontend/package.json" ]; then
    run_sed_in_place "frontend/package.json" "s/\"version\": \"[^\"]*\"/\"version\": \"$VERSION\"/"
fi

# 更新 Cargo.toml 版本号
if [ -f "backend/Cargo.toml" ]; then
    run_sed_in_place "backend/Cargo.toml" "s/^version = \"[^\"]*\"/version = \"$VERSION\"/"
fi

echo ""

# ==================== 构建前端 ====================
if [ "$BUILD_FRONTEND" = true ]; then
    echo "🎨 构建前端..."
    echo ""
    
    cd frontend
    
    require_cmd node "请先安装 Node.js，并确认 node 在当前 Bash 环境的 PATH 中。Windows/Git Bash 下如使用 nvm-windows，请检查 Git Bash 的 PATH；完整 OTA 推荐在 WSL2 中构建。"
    check_windows_node_lifecycle

    if [ -f "pnpm-lock.yaml" ]; then
        require_cmd pnpm "请先安装 pnpm。推荐: corepack enable && corepack prepare pnpm@9 --activate"
        echo "📦 同步前端依赖 (pnpm)..."
        pnpm install --frozen-lockfile
        pnpm run lint
        pnpm exec vite build
    else
        require_cmd npm "请先安装 Node.js 和 npm。"
        echo "📦 同步前端依赖 (npm)..."
        if [ -f "package-lock.json" ]; then
            npm ci
        elif [ ! -d "node_modules" ]; then
            npm install
        fi
        npm run build
    fi
    
    cd ..
    
    echo ""
    echo "✅ 前端构建完成！"
    echo "📍 输出目录: frontend/dist/"
    echo ""
fi

# ==================== 构建后端 ====================
if [ "$BUILD_BACKEND" = true ]; then
    echo "🦀 构建后端 ($TARGET)..."
    echo ""

    require_cmd cargo "请先安装 Rust 工具链。推荐: rustup + stable toolchain。"

    if command -v rustup >/dev/null 2>&1; then
        if ! rustup target list --installed | grep -qx "$TARGET"; then
            echo "❌ 错误: Rust target 未安装: $TARGET"
            echo "请执行: rustup target add $TARGET"
            exit 1
        fi
    fi

    case "$TARGET" in
        aarch64-unknown-linux-musl)
            MUSL_CC="aarch64-unknown-linux-musl-gcc"
            if ! command -v "$MUSL_CC" >/dev/null 2>&1; then
                echo "❌ 错误: 未找到 $MUSL_CC"
                echo "请安装 aarch64 Linux musl 交叉编译工具链，或使用 GitHub Actions。"
                exit 1
            fi
            export CC_aarch64_unknown_linux_musl="$MUSL_CC"
            export CARGO_TARGET_AARCH64_UNKNOWN_LINUX_MUSL_LINKER="$MUSL_CC"
            ;;
        x86_64-unknown-linux-musl)
            if command -v x86_64-unknown-linux-musl-gcc >/dev/null 2>&1; then
                MUSL_CC="x86_64-unknown-linux-musl-gcc"
            elif [[ "$(uname -s)" == "Linux" && "$(uname -m)" == "x86_64" ]] \
                && command -v musl-gcc >/dev/null 2>&1; then
                MUSL_CC="musl-gcc"
            else
                echo "❌ 错误: 未找到 x86_64 musl 编译器"
                echo "请安装 x86_64-unknown-linux-musl-gcc；x86_64 Linux 也可安装 musl-tools。"
                exit 1
            fi
            export CC_x86_64_unknown_linux_musl="$MUSL_CC"
            export CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER="$MUSL_CC"
            ;;
    esac
    
    # 设置交叉编译环境变量
    export SQLITE3_STATIC=1
    export LIBSQLITE3_SYS_USE_PKG_CONFIG=0

    # 构建
    cargo build --release --target "$TARGET" -p simadmin

    BINARY_PATH="target/$TARGET/release/simadmin"

    echo ""
    echo "✅ 后端构建完成！"
    echo "📍 二进制文件:"
    ls -lh "$BINARY_PATH"
    
    # UPX 压缩
    if [ "$USE_UPX" = true ]; then
        echo ""
        echo "UPX 压缩..."
    
        if ! command -v upx &> /dev/null; then
            echo "⚠️  未找到 upx，跳过压缩。需要强制压缩时请先安装 upx。"
        else
            BEFORE_SIZE=$(stat -f%z "$BINARY_PATH" 2>/dev/null || stat -c%s "$BINARY_PATH" 2>/dev/null)
            upx --best --lzma "$BINARY_PATH"
            AFTER_SIZE=$(stat -f%z "$BINARY_PATH" 2>/dev/null || stat -c%s "$BINARY_PATH" 2>/dev/null)
            if command -v bc >/dev/null 2>&1; then
                RATIO=$(echo "scale=1; 100 - ($AFTER_SIZE * 100 / $BEFORE_SIZE)" | bc)
                echo "压缩完成！节省: ${RATIO}%"
            else
                echo "压缩完成！"
            fi
            ls -lh "$BINARY_PATH"
        fi
    fi
    
    echo ""
    echo "📋 文件信息:"
    if command -v file >/dev/null 2>&1; then
        file "$BINARY_PATH"
    else
        echo "未找到 file 命令，跳过文件类型显示。"
    fi
fi

# ==================== 生成 OTA 包 ====================
if [ "$SKIP_OTA" = false ] && [ "$BUILD_BACKEND" = true ] && [ "$BUILD_FRONTEND" = true ]; then
    echo ""
    echo "=========================================="
    echo "  生成 OTA 更新包"
    echo "=========================================="
    echo ""
    
    BINARY_PATH="target/$TARGET/release/simadmin"
    FRONTEND_DIR="frontend/dist"
    
    # 检查构建产物
    if [ ! -f "$BINARY_PATH" ]; then
        echo "跳过 OTA: 后端二进制不存在"
    elif [ ! -d "$FRONTEND_DIR" ]; then
        echo "跳过 OTA: 前端构建产物不存在"
    else
        # 获取 Git commit
        if command -v git &> /dev/null && [ -d ".git" ]; then
            COMMIT=$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")
        else
            COMMIT="unknown"
        fi
        
        # 构建时间
        BUILD_TIME=$(TZ=Asia/Shanghai date +"%Y-%m-%dT%H:%M:%S+08:00")
        
        # 目标架构
        ARCH="$TARGET"
        
        # 创建临时目录
        require_cmd mktemp "请安装 mktemp/coreutils，或在 WSL2/Linux/macOS 中运行构建脚本。"
        require_cmd tar "请安装 tar。"
        require_cmd find "请安装 findutils。"
        require_cmd sort "请安装 coreutils。"
        if ! is_macos; then
            require_cmd md5sum "请安装 coreutils，确保 md5sum 可用。"
        fi

        OTA_TMP=$(mktemp -d)
        trap "rm -rf $OTA_TMP" EXIT
        
        echo "版本: $VERSION"
        echo "Commit: $COMMIT"
        echo "构建时间: $BUILD_TIME"
        echo ""
        
        # 复制后端二进制
        echo "复制后端二进制..."
        cp "$BINARY_PATH" "$OTA_TMP/simadmin"
        chmod 755 "$OTA_TMP/simadmin"
        
        # 计算二进制 MD5
        if is_macos; then
            BINARY_MD5=$(md5 -q "$OTA_TMP/simadmin")
        else
            BINARY_MD5=$(md5sum "$OTA_TMP/simadmin" | cut -d' ' -f1)
        fi
        echo "  二进制 MD5: $BINARY_MD5"
        
        # 复制前端文件
        echo "复制前端文件..."
        mkdir -p "$OTA_TMP/www"
        cp -r "$FRONTEND_DIR"/* "$OTA_TMP/www/"

        # 计算前端 MD5
        if is_macos; then
            FRONTEND_MD5=$(find "$OTA_TMP/www" -type f -exec md5 -q {} \; | sort | tr '\n' '\n' | md5 -q)
        else
            FRONTEND_MD5=$(find "$OTA_TMP/www" -type f -exec md5sum {} \; | cut -d' ' -f1 | sort | md5sum | cut -d' ' -f1)
        fi
        echo "  前端 MD5: $FRONTEND_MD5"
        
        # 生成 meta.json
        cat > "$OTA_TMP/meta.json" << EOF
{
    "version": "$VERSION",
    "commit": "$COMMIT",
    "build_time": "$BUILD_TIME",
    "binary_md5": "$BINARY_MD5",
    "frontend_md5": "$FRONTEND_MD5",
    "arch": "$ARCH"
}
EOF
        
        # 创建输出目录
        mkdir -p release
        
        # 打包
        OTA_FILE="release/simadmin_${VERSION}_${TARGET}.tar.gz"
        echo "打包 OTA..."
        cd "$OTA_TMP"
        tar -czf - meta.json simadmin www > "$OLDPWD/$OTA_FILE"
        cd "$OLDPWD"
        
        # 显示结果
        echo ""
        echo "OTA 更新包生成完成!"
        echo "输出: $OTA_FILE"
        ls -lh "$OTA_FILE"
        
        # 计算包的 MD5
        if is_macos; then
            OTA_MD5=$(md5 -q "$OTA_FILE")
        else
            OTA_MD5=$(md5sum "$OTA_FILE" | cut -d' ' -f1)
        fi
        echo "OTA 包 MD5: $OTA_MD5"
    fi
fi

echo ""
echo "=========================================="
echo "部署命令: ./scripts/deploy.sh"
echo "=========================================="
