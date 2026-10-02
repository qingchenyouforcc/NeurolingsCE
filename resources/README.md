# 内置资源

`DefaultMascot/` 包含内置桌宠的 `info.json`、行为 XML 和 PNG 帧。运行时通过 Rust
资源包校验器读取它们，外部导入仍沿用相同的路径、大小和归档条数限制。

`bubbles.txt` 是气泡文本候选，按 UTF-8 行读取；空行不会生成气泡。
