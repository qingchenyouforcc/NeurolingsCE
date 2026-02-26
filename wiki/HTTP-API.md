# HTTP API

NeurolingsCE 内置一个 HTTP REST API，运行在 `http://127.0.0.1:32456`。可用于外部程序控制看板娘。

> NeurolingsCE includes a built-in HTTP REST API running at `http://127.0.0.1:32456` for external program control.

## 基本信息 / Basic Info

| 项目 | 值 |
|------|-----|
| Base URL | `http://127.0.0.1:32456/shijima/api/v1` |
| 协议 | HTTP（仅本地回环） |
| 端口 | 32456 |
| 数据格式 | JSON |
| 线程 | 独立线程运行，通过 tick 回调线程安全地与主线程通信 |

## 单实例检查 / Single Instance Check

NeurolingsCE 启动时会先 ping API：

```
GET /shijima/api/v1/ping
```

- 如果收到响应 → 说明已有实例运行，新进程退出
- 如果无响应（连接超时 500ms）→ 正常启动

## API 端点 / Endpoints

### GET /mascots

获取当前屏幕上所有活跃的看板娘实例。

> Returns a list of all active mascots on screen.

**响应示例 / Sample Response:**

```json
{
    "mascots": [
        {
            "active_behavior": "ClimbIEWall",
            "anchor": {
                "x": 67.2,
                "y": 225.64
            },
            "data_id": 0,
            "id": 35,
            "name": "Default Mascot"
        },
        {
            "active_behavior": "Fall",
            "anchor": {
                "x": 368,
                "y": 863
            },
            "data_id": 0,
            "id": 36,
            "name": "Default Mascot"
        }
    ]
}
```

**字段说明 / Fields:**

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | int | 看板娘实例的唯一 ID |
| `data_id` | int | 对应的 `MascotData` ID |
| `name` | string | 看板娘名称 |
| `active_behavior` | string\|null | 当前执行的行为名称 |
| `anchor.x` | double | 看板娘锚点 X 坐标（屏幕坐标） |
| `anchor.y` | double | 看板娘锚点 Y 坐标（屏幕坐标） |

---

### POST /mascots

生成一只新的看板娘。必须指定 `name` 或 `data_id` 之一。

> Spawn a new mascot. One of `name` or `data_id` must be specified.

**请求格式 / Request Format:**

```json
{
    "name": "string",
    "data_id": "int",
    "anchor": {
        "x": "double",
        "y": "double"
    },
    "behavior": "string"
}
```

| 字段 | 必须 | 说明 |
|------|------|------|
| `name` | 二选一 | 看板娘名称 |
| `data_id` | 二选一 | 看板娘数据 ID |
| `anchor` | 可选 | 初始位置 |
| `behavior` | 可选 | 初始行为 |

**请求示例 / Sample Request:**

```json
{
    "name": "Default Mascot",
    "anchor": {
        "x": 150,
        "y": 150
    },
    "behavior": "SplitIntoTwo"
}
```

**响应示例 / Sample Response:**

```json
{
    "mascot": {
        "active_behavior": null,
        "anchor": {
            "x": 150,
            "y": 150
        },
        "data_id": 0,
        "id": 40,
        "name": "Default Mascot"
    }
}
```

---

### DELETE /mascots

销毁所有看板娘实例。

> Dismiss all mascots.

---

### GET /mascots/:id

获取指定 ID 的看板娘信息。

> Get data for a specific mascot by ID.

**响应示例 / Sample Response:**

```json
{
    "mascot": {
        "active_behavior": "ClimbIEWall",
        "anchor": {
            "x": 67.2,
            "y": 225.64
        },
        "data_id": 0,
        "id": 35,
        "name": "Default Mascot"
    }
}
```

---

### PUT /mascots/:id

修改指定看板娘的状态（位置、行为）。

> Alter the state of an existing mascot.

**请求格式 / Request Format:**

```json
{
    "id": "int",
    "anchor": {
        "x": "double",
        "y": "double"
    },
    "behavior": "string"
}
```

**请求示例 / Sample Request:**

```json
{
    "id": 4,
    "anchor": {
        "x": 150,
        "y": 150
    },
    "behavior": "SplitIntoTwo"
}
```

**响应示例 / Sample Response:**

```json
{
    "mascot": {
        "active_behavior": "SitDown",
        "anchor": {
            "x": 150,
            "y": 150
        },
        "data_id": 79,
        "id": 4,
        "name": "Jenny"
    }
}
```

---

### GET /loadedMascots

获取所有已加载的看板娘资源包列表（可用于生成的看板娘）。

> Returns a list of all loaded mascot data that can be spawned.

**响应示例 / Sample Response:**

```json
{
    "loaded_mascots": [
        {
            "id": 0,
            "name": "Default Mascot"
        },
        {
            "id": 79,
            "name": "Jenny"
        },
        {
            "id": 78,
            "name": "niko"
        }
    ]
}
```

---

### GET /loadedMascots/:id

获取指定 ID 的已加载看板娘资源包信息。

> Returns information about a specific loaded mascot data.

**响应示例 / Sample Response:**

```json
{
    "loaded_mascot": {
        "id": 79,
        "name": "Jenny"
    }
}
```

---

### GET /loadedMascots/:id/preview.png

获取指定看板娘的预览图像。

> Returns the preview image for a loaded mascot.

**响应**: PNG 图片

---

### GET /ping

心跳检查端点，用于单实例检测。

> Health check endpoint, used for single-instance detection.

---

## 使用示例 / Usage Examples

### cURL

```bash
# 获取所有活跃看板娘
curl http://127.0.0.1:32456/shijima/api/v1/mascots

# 生成一只看板娘
curl -X POST http://127.0.0.1:32456/shijima/api/v1/mascots \
  -H "Content-Type: application/json" \
  -d '{"name":"Default Mascot","anchor":{"x":200,"y":200}}'

# 移动看板娘
curl -X PUT http://127.0.0.1:32456/shijima/api/v1/mascots/35 \
  -H "Content-Type: application/json" \
  -d '{"anchor":{"x":500,"y":300}}'

# 销毁所有看板娘
curl -X DELETE http://127.0.0.1:32456/shijima/api/v1/mascots

# 获取预览图
curl -o preview.png http://127.0.0.1:32456/shijima/api/v1/loadedMascots/0/preview.png
```

### Python

```python
import requests

BASE = "http://127.0.0.1:32456/shijima/api/v1"

# 列出所有看板娘
mascots = requests.get(f"{BASE}/mascots").json()
print(mascots)

# 生成看板娘
resp = requests.post(f"{BASE}/mascots", json={
    "name": "Default Mascot",
    "anchor": {"x": 300, "y": 200}
})
print(resp.json())
```

## 线程安全说明 / Thread Safety

HTTP API 在独立线程运行。涉及看板娘状态修改的操作（如 spawn、delete、modify）会通过 `ShijimaManager::onTickSync()` 回调机制在主线程的下一次 tick 中执行，确保线程安全。

```
HTTP 线程                          主线程 (tick loop)
    │                                  │
    ├── onTickSync(callback) ──────►   │
    │   (加锁, 等待)                    ├── 执行 callback
    │   ◄──────── notify_all ──────────┤
    ├── 返回响应                        │
```

## 相关页面 / Related Pages

- [快速开始](Getting-Started) — 如何使用 CLI 模式与 API 交互
- [架构概览](Architecture) — ShijimaHttpApi 的位置和设计
