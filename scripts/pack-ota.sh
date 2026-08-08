#!/bin/bash

# 打包 OTA 更新包
# 输出: release/simadmin_{version}_{target}.tar.gz

set -e

# 切换到项目根目录
cd "$(dirname "$0")/.."

TARGET="${TARGET:-aarch64-unknown-linux-musl}"
for arg in "$@"; do
    case "$arg" in
        --target=aarch64|--target=arm64|--target=aarch64-unknown-linux-musl)
            TARGET="aarch64-unknown-linux-musl"
            ;;
        --target=x86_64|--target=amd64|--target=x86_64-unknown-linux-musl)
            TARGET="x86_64-unknown-linux-musl"
            ;;
        --help|-h)
            echo "用法: ./scripts/pack-ota.sh [--target=aarch64|x86_64]"
            exit 0
            ;;
        *)
            echo "❌ 错误: 未知选项: $arg" >&2
            exit 1
            ;;
    esac
done

case "$TARGET" in
    aarch64|arm64) TARGET="aarch64-unknown-linux-musl" ;;
    x86_64|amd64) TARGET="x86_64-unknown-linux-musl" ;;
    aarch64-unknown-linux-musl|x86_64-unknown-linux-musl) ;;
    *)
        echo "❌ 错误: 不支持的构建目标: $TARGET" >&2
        exit 1
        ;;
esac

echo "=========================================="
echo "  打包 OTA 更新包"
echo "=========================================="
echo ""

# 读取版本号
VERSION_FILE="VERSION"
if [ -f "$VERSION_FILE" ]; then
    VERSION=$(cat "$VERSION_FILE" | tr -d '[:space:]')
else
    echo "❌ 错误: VERSION 文件不存在"
    exit 1
fi

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

# 检查构建产物
BINARY_PATH="backend/target/$TARGET/release/simadmin"
FRONTEND_DIR="frontend/dist"

if [ ! -f "$BINARY_PATH" ]; then
    echo "❌ 错误: 后端二进制不存在: $BINARY_PATH"
    echo "请先运行: ./scripts/build.sh"
    exit 1
fi

if [ ! -d "$FRONTEND_DIR" ]; then
    echo "❌ 错误: 前端构建产物不存在: $FRONTEND_DIR"
    echo "请先运行: ./scripts/build.sh"
    exit 1
fi

# 创建临时目录
OTA_TMP=$(mktemp -d)
trap "rm -rf $OTA_TMP" EXIT

echo "📦 版本: $VERSION"
echo "📝 Commit: $COMMIT"
echo "🕐 构建时间: $BUILD_TIME"
echo ""

# 复制后端二进制
echo "📋 复制后端二进制..."
cp "$BINARY_PATH" "$OTA_TMP/simadmin"
chmod 755 "$OTA_TMP/simadmin"

# 计算二进制 MD5
if [[ "$OSTYPE" == "darwin"* ]]; then
    BINARY_MD5=$(md5 -q "$OTA_TMP/simadmin")
else
    BINARY_MD5=$(md5sum "$OTA_TMP/simadmin" | cut -d' ' -f1)
fi
echo "   MD5: $BINARY_MD5"

# 复制前端文件
echo "📋 复制前端文件..."
mkdir -p "$OTA_TMP/www"
cp -r "$FRONTEND_DIR"/* "$OTA_TMP/www/"

# 计算前端 MD5（所有文件的 hash，与 Rust 验证逻辑一致）
# 方式：每个文件的 MD5 排序后，用换行符连接，再计算整体 MD5
echo "📋 计算前端 MD5..."
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS: 收集所有 MD5，排序，每行一个，然后计算整体 MD5
    FRONTEND_MD5=$(find "$OTA_TMP/www" -type f -exec md5 -q {} \; | sort | tr '\n' '\n' | md5 -q)
else
    # Linux: 同样的逻辑
    FRONTEND_MD5=$(find "$OTA_TMP/www" -type f -exec md5sum {} \; | cut -d' ' -f1 | sort | md5sum | cut -d' ' -f1)
fi
echo "   MD5: $FRONTEND_MD5"
EDITION="${EDITION:-${VARIANT:-standard}}"
for arg in "$@"; do
    case "$arg" in
        --wfc|wfc) EDITION="wfc" ;;
        --edition=*|--variant=*) EDITION="${arg#*=}" ;;
    esac
done

# 生成 meta.json
echo "📋 生成 meta.json ( edition: $EDITION)..."
cat > "$OTA_TMP/meta.json" << EOF
{
    "version": "$VERSION",
    "commit": "$COMMIT",
    "build_time": "$BUILD_TIME",
    "binary_md5": "$BINARY_MD5",
    "frontend_md5": "$FRONTEND_MD5",
    "arch": "$ARCH",
    "edition": "$EDITION"
}
EOF

cat "$OTA_TMP/meta.json"
echo ""

# 创建输出目录
mkdir -p release

# 打包
OTA_FILE="release/simadmin_${VERSION}_${TARGET}.tar.gz"
echo "📦 打包 OTA 更新包..."
cd "$OTA_TMP"
tar -czf - meta.json simadmin www > "$OLDPWD/$OTA_FILE"
cd "$OLDPWD"

# 显示结果
echo ""
echo "=========================================="
echo "✅ OTA 更新包打包完成！"
echo "=========================================="
echo ""
echo "📍 输出文件: $OTA_FILE"
ls -lh "$OTA_FILE"
echo ""
echo "📋 包内容:"
tar -tzf "$OTA_FILE" | head -20
echo "..."
echo ""

# 计算包的 MD5
if [[ "$OSTYPE" == "darwin"* ]]; then
    OTA_MD5=$(md5 -q "$OTA_FILE")
else
    OTA_MD5=$(md5sum "$OTA_FILE" | cut -d' ' -f1)
fi
echo "📝 OTA 包 MD5: $OTA_MD5"
