# NeurolingsCE Rust 重写进度

## 当前阶段

- [x] 创建独立 workspace 和 `main` 分支
- [ ] 建立兼容性基线
- [ ] 完成 `api`
- [ ] 完成 `assets`
- [ ] 完成 `engine`
- [ ] 完成 `runtime`
- [ ] 完成传输层
- [ ] 完成平台与 GUI
- [ ] 完成外围服务和打包

## 约束

- 只使用当前工作树作为行为与资源来源，不读取 Git 历史。
- 每个小功能提交前必须通过对应测试和 workspace 编译。
- 不把 C++/Qt/Duktape 业务代码复制到新仓库。
