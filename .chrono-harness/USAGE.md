# chrono-harness-pr 使用说明

组件的用途、开发仓库和示例见 [Chrono Harness 用户与开发者入口](https://github.com/chrono-harness/chrono-harness-main)。

`chrono-pr` 通过 GitHub CLI 打开 PR、查询指定提交、等待检查。每次远端查询都读取 GitHub。可用 `pr-reminders` 离线查看当前工作树里由本工具创建、仍待跟进的 PR。

开发分支为 `dev`。日常 PR 合入开发分支，正式版从 `main` 发布。

首次检出运行 `mise trust` 和 `mise run harness-setup`。工具版本见 [.config/mise/conf.d/chrono-harness.toml](../.config/mise/conf.d/chrono-harness.toml)。

根 [CLAUDE.md](../CLAUDE.md) 包含本仓接纳的基础指令全文。规则选择在 `.chrono-harness/rules.json`。开始任务时运行 `mise run harness-setup` 刷新根指南，成功后重新阅读本仓的 `AGENTS.md` 或 `CLAUDE.md`。仓库追加说明见根 `CLAUDE.md`。


本地检查使用 runner 登记的原件供单元报告、汇总和排错读取。保留期限由本仓 `.chrono-harness/retention.json` 决定，初始窗口为 24 小时。首次启用时运行 `mise run harness-setup` 初始化缺失的策略，并将该文件提交到本仓。以后由宿主管理，切换模板选项时仍可供其他生产者使用。需要长期使用的报告和原件应在窗口内复制到持久存储。维护回收已经到期且引用已释放的原件。启用前生成的未登记文件由原生产者负责。

本仓选择下列成功的本地完整检查报告。到期后，runner 会核对读者和交接义务，再释放报告的固定路径和引用。

- `.chrono-harness/state/collection/check.json`



运行 `mise run harness-maintain-checks` 进行一次维护。长期检出可在本机用户的 crontab 中加入下列条目，将仓库和 mise 路径换为本机绝对路径。定时器每小时唤起同一入口，启用后用原件的实际消失核对回收。在删除条目之前应保全有价值的结果，并完成剩余清理。

```cron
0 * * * * cd '/absolute/path/to/repository' && '/absolute/path/to/mise' run harness-maintain-checks >/dev/null
```



用 `mise exec -- chrono-pr pr-reminders` 查看本地提醒，再运行 `mise exec -- chrono-pr pr-status --pr NUMBER --head-sha SHA` 查询 GitHub。

## 本仓验证

产品位于 [`crates/pr`](../crates/pr)。在仓库根目录运行 `mise exec -- ./check.sh` 执行本仓的检查。 本地和 GitHub 使用同一份 Git DELTA、证明根与原生测试登记。macOS 单元执行 PR 状态的生产与独立测试证明、原生 PR 行为和发布配置检查，Linux 单元验证进程组退出，汇总作业读取原件。
