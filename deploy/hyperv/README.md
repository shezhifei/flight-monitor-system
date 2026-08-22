# Hyper-V 方案说明

本目录保留的是早期 Hyper-V 切分脚本，当前已经不再是推荐部署路径。

当前默认方案请直接看：

- [部署主文档](/C:/flight-monitor-system/docs/DEPLOYMENT.md)
- [Docker 一键启动脚本](/C:/flight-monitor-system/deploy/docker/Start-FlightMonitorDocker.bat)

## 当前状态

- Hyper-V 方案仅作为历史预研与容量拆分参考保留。
- 当前仓库已经以 Docker Desktop 单机分布式拓扑作为事实上的主路径。
- 如果你只是要在本机稳定运行系统，不要从本目录开始。

## 本目录包含什么

- [flight-monitor-hyperv-plan.json](/C:/flight-monitor-system/deploy/hyperv/flight-monitor-hyperv-plan.json)：历史切分计划样例
- [Test-FlightMonitorHostCapacity.ps1](/C:/flight-monitor-system/deploy/hyperv/Test-FlightMonitorHostCapacity.ps1)：宿主机资源校验脚本
- [New-FlightMonitorDistributedLab.ps1](/C:/flight-monitor-system/deploy/hyperv/New-FlightMonitorDistributedLab.ps1)：批量创建 VM 壳子的脚本
- [Invoke-FlightMonitorHostProvision.ps1](/C:/flight-monitor-system/deploy/hyperv/Invoke-FlightMonitorHostProvision.ps1)：宿主机预配置脚本

## 适用场景

只有在以下场景才建议继续使用 Hyper-V 路径：

- 你明确需要演练多台 Linux VM 的网络隔离
- 你要验证 VM 级 CPU / 内存切分
- 你准备自己手工完成 PostgreSQL 主从、Redis 与 Nginx 的多机安装

## 限制

- 这不是当前维护重点
- 这不是一键可用方案
- 即使拆成多台 VM，只要还在同一台物理主机上，也不构成真正高可用
