# NeurolingsCE Rust 重写进度

## 当前阶段

- [x] 创建独立 workspace 和 `main` 分支
- [x] 建立兼容性基线
- [x] 完成 `api` 协议模型
- [x] 完成 `assets` 安全路径、元数据与 ZIP 包
- [x] 完成 `engine` 表达式、XML 动作与行为运行时
- [x] 完成 `runtime` 会话与固定 tick
- [x] 完成 IPC、HTTP 路由和 CLI
- [x] 完成平台抽象与 egui GUI 基础层
- [x] 完成认证、商店、更新和投稿外围服务
- [ ] 完成跨平台安装包、资源迁移和发布验证

## 约束

- 只使用当前工作树作为行为与资源来源，不读取 Git 历史。
- 每个小功能提交前必须通过对应测试和 workspace 编译。
- 不把 C++/Qt/Duktape 业务代码复制到新仓库。
