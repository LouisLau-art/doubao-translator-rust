# Doubao Translator (Rust)

![Rust](https://img.shields.io/badge/rust-1.70%2B-dea584.svg)
![Axum](https://img.shields.io/badge/axum-0.7-0f6e66.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

极简、高性能的网页翻译器：Rust 后端 + 原生前端，专注速度和低内存占用。对接火山引擎豆包翻译 API（ARK）。

## Features
- Rust + axum 后端，低内存占用
- Markdown + LaTeX 渲染（本地库）
- 自动翻译 + 历史记录（localStorage）
- 速率限制 + LRU 缓存
- systemd 静默后台启动，开机自启

## Quick Start
```bash
git clone <repo>
cd doubao-translator-rust
cp .env.example .env
# 编辑 .env 填写 ARK_API_KEY
make dev
```
浏览器访问：`http://localhost:5000`

## Build & Run
```bash
make build-prod
./target/release/translator
```

## systemd (静默 + 自启)
```bash
make install-service
# 查看状态
sudo systemctl status doubao-translator.service
```

## Configuration
`.env` 示例：
```env
ARK_API_KEY=your_ark_api_key_here
ARK_API_URL=https://ark.cn-beijing.volces.com/api/v3/responses
PORT=5000
CACHE_TTL=3600
CACHE_MAX_SIZE=1000
MAX_TEXT_LENGTH=5000
RATE_LIMIT_RPM=30
```

## Project Structure
```
/ src/                 # Rust 后端
/ static/              # 前端
  / libs/              # 本地依赖 (marked, MathJax)
/ systemd/             # systemd 服务文件
/ scripts/             # 安装/卸载脚本
```

## License
MIT
