# chrono-harness-pr

绑定提交的 GitHub PR 命令。

使用 `cargo build --locked --manifest-path crates/pr/Cargo.toml` 构建。安装 Verus 0.2026.04.24.f8e1704，运行 `VERUS=/path/to/verus ./check.sh` 检查格式、构建、测试及证明。

源码来自 [固定版本的原实现](https://github.com/ChronoAIProject/chrono-harness/tree/5f6892dfa2f73675a84e4ebdc3b06eb83b824d6e/crates/pr)。

## Install a released binary

Choose a tag from this repository's [releases](https://github.com/chrono-harness/chrono-harness-pr/releases). Download the matching `chrono-pr-macos-aarch64` or `chrono-pr-linux-x86_64` asset and `SHA256SUMS`. Verify its SHA-256 against the matching entry and install it as `chrono-pr` outside your checkout. Each tag identifies its own source and checksums. The CLI uses your existing GitHub CLI authentication.
