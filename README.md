# chrono-harness-pr

`chrono-pr` 通过 GitHub CLI 打开 PR、查询指定提交、等待检查。每次远端查询都读取 GitHub。可用 `pr-reminders` 离线查看当前工作树里由本工具创建、仍待跟进的 PR。

## 安装与更新

从本仓库选定一个固定的 [Release](https://github.com/chrono-harness/chrono-harness-pr/releases)，下载平台对应的 `chrono-pr-macos-aarch64` 或 `chrono-pr-linux-x86_64` 和该版本的 `SHA256SUMS`。核对文件 SHA-256，在工作树以外安装为可执行的 `chrono-pr`。升级和回退均选择所需标签，按同样方式校验、替换该命令。使用已有的 `gh` 登录身份。

## 配置与使用

在工作树的 `.chrono-harness/pr.json` 填写仓库、目标分支、分支要求的检查名及调用上限。也可通过 `--host-root` 指定工作树，或通过 `--config` 指定其下的配置文件。

```json
{
  "schema": "chrono-pr-config/v3",
  "repository": "chrono-harness/chrono-harness-runner",
  "base": "main",
  "gh": "gh",
  "required_checks": ["check"],
  "api_timeout_seconds": 60,
  "watch_timeout_seconds": 3600,
  "output_limit_bytes": 1048576
}
```

```sh
chrono-pr pr-reminders
chrono-pr pr-open --head my-branch --head-sha "$head_sha" --title "$title" --body-file body.md
chrono-pr pr-status --pr 42 --head-sha "$head_sha"
chrono-pr pr-watch --pr 42 --head-sha "$head_sha" < completion.jsonl
```

`pr-open` 支持 `--draft`，复用已有匹配的 PR。创建得到有效编号后即保存提醒，回读失败也可按编号重新查询。远端命令的标准输出为 `chrono-pr-result/v3` JSON，含提交、检查、合并身份及实际子进程的 argv、退出状态和有界输出字节。收到信号时，进程回执的 `exit_signal` 记录实际信号，`exit_code` 为 `null`。命令退出码分别为 0（成功）、1（检查失败或关闭）、2（错误或未知）、3（待定）、4（提交已变化）。如出现 `reminder_error`，远端结果仍可使用，本次本地提醒更新失败。

提醒存放于工作树的 `.chrono-harness/pr-reminders.sqlite3`，应加入宿主的 Git 忽略规则。SQLite 在提交删除时把空闲页交还文件系统。已有提醒库在首次使用时转换一次格式。查询其他人的 PR 不会将其加入提醒。查到关闭或合并即移除；每次使用本工具时清除创建已满 30 天的提醒。离线调用 `pr-reminders` 也会清理已过期记录。工作树被宿主回收时整个本地库随之删除。需要长期保存的查询结果可直接从 stdout 写入自己的持久存储。

主检出的维护执行者可调用 `mise exec -- chrono-pr pr-reminders`，在本地读取提醒并删除过期行。检查原件的窗口和维护入口见[开发机检查原件](.chrono-harness/USAGE.md)。

`pr-watch` 先查询 GitHub 上的目标 PR、head、base 与合并状态。固定 head 上已有冲突时直接返回 `conflict`（退出 1）。必需检查仍在运行时，向标准输入逐行发送原始 GitHub webhook JSON。支持 `workflow_run.completed`、`pull_request.closed`、`pull_request.synchronize` 和 head 或 base 分支的 `push`。来自 fork 的 head 推送需要该 fork 的事件通道，目标仓的 synchronize 事件也可唤醒。无关仓库、无关分支或旧提交的事件会被略过。命令在匹配事件到达后重新读取 PR；head 已变返回 `stale`（退出 4），base 已变且确认冲突返回 `conflict`，base 已变且合并状态仍待计算时返回 `changed`（退出 3）。JSON 的 `observed_base`、`observed_head`、`head_repository` 和 `mergeable` 给出 GitHub 本次观察值。通知通道关闭、超时或查询失败时返回 `unknown`。

旧预览版 v0.1.0-alpha.1 使用 v2 配置与操作账本。升级时将配置改为 v3，删除 `state_directory`、`state_producer`，重新查询指定 PR。旧预览版及其原校验文件仍可从对应标签取得。

### 等待 push 的 CI 进展

用 GitHub Actions 运行页面中的数字 ID 启动一次等待。可用 `--head-sha SHA` 同时核对这次 push 的提交。

```sh
chrono-pr run-watch --run 857 --head-sha "$head_sha" < github-events.jsonl
```

调用方先接入已有的 GitHub 通知流，以每行一个原始 JSON 正文向 stdin 送入 `workflow_run` 和 `workflow_job` 事件。客户端先读取指定 push run。已经完成时直接返回，进行中时阻塞，收到同仓同 run 的事件后回读 GitHub。当前 attempt、运行状态、作业或步骤的观察发生变化即返回。无关、重复且未造成新观察的事件继续等待。GitHub 提供作业级通知，步骤进展在这些通知触发回读时可见。

输出 `chrono-run-result/v1` 包含 run ID、提交、attempt、作业、步骤及原始查询回执。`changed`（3）表示本次等待已经取得进展，CI 可以仍在运行；需要下一次变化时，用同一 run ID 再次调用并连接仍在接收事件的通道。`passed`（0）与 `failed`（1）给出 CI 的终态，`stale`（4）表示所给 SHA 不同。通道丢失或期限届满返回 `unknown`（2）。

每次调用为 gh 建立专属进程组。启动器退出后，macOS 通过 kqueue、Linux 通过 pidfd 等候组内仍在工作的成员，包括已关闭标准流的成员。收到退出事件后重新核对组内成员，调用超时会取消原进程组。无法确认进程组已经结束时，该次请求返回 unknown。

## 构建、测试与证明

安装本仓固定的 Rust 工具链和 Verus `0.2026.04.24.f8e1704`，运行 `VERUS=/path/to/verus ./check.sh`。所有测试、命令替身和证明根都位于本仓的 `crates/pr` 产品内。构建发行二进制使用 `cargo build --release --locked --manifest-path crates/pr/Cargo.toml`。

## 预览和正式发布

`.chrono-harness/release.json` 登记目标版本、消费仓库的分支、安装清单和实际验收作业。dev 的 push 检查成功后，Release chrono-pr 工作流为该提交创建 `v<version>-alpha.<check run_number>`，在 macOS arm64 和 Linux x86_64 编译并运行程序，发布两个二进制及 SHA256SUMS。

示例仓库先锁定预览的两个 URL 和摘要，并在落地提交上通过登记的实际业务检查。收到该下游完成事件后，在本仓 dev 触发 `gh workflow run release.yml --ref dev`。工作流自行解析下游当前提交、对应 push 作业、安装版本、检查和两个平台的摘要。通过后将 main 快进到这个预览的同一提交，并把校验后的原资产发布为正式版。main 的已成功落地检查也会运行同一判定。跨仓触发方只需发一次事件。未安装事件转发的宿主可用上述命令唤醒。

已公开资产不可覆盖。工作流重试会检查同一来源和现有资产，只有未完成的草稿可继续上传。发布策略只在本仓 GitHub Actions 使用。
