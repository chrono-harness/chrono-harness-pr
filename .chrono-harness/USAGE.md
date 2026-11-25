# chrono-harness-pr 使用说明

组件的用途、开发仓库和示例见 [Chrono Harness 用户与开发者入口](https://github.com/chrono-harness/chrono-harness-main)。

`chrono-pr` 通过 GitHub CLI 打开 PR、查询指定提交、等待检查。每次远端查询都读取 GitHub。可用 `pr-reminders` 离线查看当前工作树里由本工具创建、仍待跟进的 PR。

开发分支为 `dev`。日常 PR 合入开发分支，正式版从 `main` 发布。

首次检出运行 `mise trust` 和 `mise run harness-setup`。本仓默认安装策略为 `pinned`。固定配方读取 [.config/mise/conf.d/chrono-harness.toml](../.config/mise/conf.d/chrono-harness.toml)。模板答案 `tool_versions` 可以为每个工具选择 stable、preview 或精确标签。临时运行 `CHRONO_TOOL_CHANNEL=preview mise run harness-setup` 会为已选组织工具安装各自最新测试版，也可以把 `preview` 换成 `stable` 或精确的 `v` 标签。再次运行普通安装即可恢复本仓选择。本仓的专用 CI 通过 .chrono-harness/ci/install.sh 安装登记的检查工具。各分支、标签和 detached HEAD 使用相同规则。

根 [CLAUDE.md](../CLAUDE.md) 包含本仓接纳的基础指令全文。规则选择在 `.chrono-harness/rules.json`。开始任务时运行 `mise run harness-setup` 刷新根指南，成功后重新阅读本仓的 `AGENTS.md` 或 `CLAUDE.md`。仓库追加说明见根 `CLAUDE.md`。


本地检查使用 runner 登记的原件供单元报告、汇总和排错读取。保留期限由本仓 `.chrono-harness/retention.json` 决定，初始窗口为 24 小时。首次启用时运行 `mise run harness-setup` 初始化缺失的策略，并将该文件提交到本仓。以后由宿主管理，切换模板选项时仍可供其他生产者使用。需要长期使用的报告和原件应在窗口内复制到持久存储。维护回收已经到期且引用已释放的原件。启用前生成的未登记文件由原生产者负责。

本仓选择下列成功的本地完整检查报告。到期后，runner 会核对读者和交接义务，再释放报告的固定路径和引用。

- `.chrono-harness/state/collection/check.json`





原生检查入口用 `chrono-host-bootstrap native-scratch run PROGRAM [ARG ...]` 启动编译或测试。子程序从 `CHRONO_NATIVE_SCRATCH` 取得本次专用目录。成功且读者退出后即可回收；失败或中断的目录保留 86400 秒，仍持租约时继续保全。需要留存的诊断应在窗口内转存并核对摘要。


运行 `mise run harness-maintain-checks` 可执行一轮本仓已选的维护。宿主已将此命令接入事件维护服务时，业务事件唤醒服务，由持有协调者独占租约的进程调用它。模板只生成这条命令。保留期届满且读者已释放的产物，由下一次成功处理该目录的维护轮次回收。长期没有事件或显式维护时，已到期的产物会继续占用磁盘。



用 `mise exec -- chrono-pr pr-reminders` 查看本地提醒，再运行 `mise exec -- chrono-pr pr-status --pr NUMBER --head-sha SHA` 查询 GitHub。

## 本仓验证

产品位于 [`crates/pr`](../crates/pr)。在仓库根目录运行 `mise exec -- ./check.sh` 执行本仓的检查。 本地和 GitHub 使用同一份 Git DELTA、证明根与原生测试登记。macOS 单元执行 PR 状态的生产与独立测试证明、原生 PR 行为和发布配置检查，Linux 单元验证进程组退出，汇总作业读取原件。
