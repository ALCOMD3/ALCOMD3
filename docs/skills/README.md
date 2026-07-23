# ALCOMD3 Agent skills

本目录存放给 Agent 使用的仓库内执行清单。Agent 处理相关任务时应先阅读根级
`AGENTS.md`，再按任务类型读取对应 skill。

## 可用 skill

- [alcomd3-release](./alcomd3-release/SKILL.md)：用于发布、发布审计、版本准备、GitHub Release、updater metadata、stable/beta channel 和 release notes 相关任务。

## 使用规则

- 只要求审计时，不创建 GitHub Release、不上传 artifacts、不提交、不推送。
- 涉及实际发布时，先确认 version、channel 和授权范围。
- 默认使用 GitHub Actions 创建 Draft 并在发布后更新 updater metadata；本地 `xtask`
  构建仅作为验证和故障恢复路径。
- 发布流程的人类手册默认入口是 [../RELEASE.md](../RELEASE.md)，中文版本是
  [../RELEASE/RELEASE.zh-CN.md](../RELEASE/RELEASE.zh-CN.md)。
