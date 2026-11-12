# pbot

[English](cli-command-reference.md) · **中文**

## 名称

`pbot` —— [Plane.so](https://plane.so)（SaaS 或自托管）的命令行客户端：工作项（work item）、项目、周期、模块、标签、状态、文档、收件箱与评论。

`pbot` 是 `pbotcli` 的别名，两者是同一个二进制。

## 用法

```
pbot [全局选项] <命令> [<子命令>] [<参数>...] [选项]
```

## 说明

所有资源参数都支持**模糊解析**：可传名称、标识符（`ABC-123`）或 UUID；相近名称按相似度匹配（token-sort-ratio，阈值 60）。需要指定负责人处可用 `me` 表示当前用户；使用**服务主体**（service principal）凭证时，`me` 解析为该主体的**影子机器人成员**（见 [`pbot whoami`](#pbot-whoami)）。

跨项目查找（不带 `-p` 的 `wi show`、`wi ls`）会**跳过**当前凭证读不到的项目 —— HTTP 403（服务主体对没有授权的项目都会返回该状态）或 404 —— 并提示跳过了哪些项目，而不是在第一个项目上整体失败。只有"读取该项目工作项"被拒才构成跳过：该项目的 states / labels 被拒属于另一种失败，会中断命令，而不会把"工作项明明读得到"的项目报成"no read access"。其他错误（凭证被拒、5xx）同样中断；`wi show` 查不到时，提示中会列出本次未能覆盖的项目。

人读表格输出到 **stderr**，机读 JSON 输出到 **stdout**，因此 `pbot ... --json 2>/dev/null` 总能得到纯 JSON。读操作走磁盘缓存；写操作会使对应缓存失效；对于 API 可能"返回成功但实际未生效"的写入（日期、triage、上传），会回读校验。

## 全局选项

| 选项 | 说明 |
|---|---|
| `--json` | JSON 输出到 stdout；人读表格到 stderr。 |
| `--no-cache` | 本次命令绕过磁盘缓存。 |
| `-h, --help` | 查看任意命令的帮助。 |
| `-V, --version` | 打印版本号。 |

## 环境变量

| 变量 | 说明 |
|---|---|
| `PLANE_BASE_URL` | 实例地址，如 `https://api.plane.so`。 |
| `PLANE_API_KEY` | 服务令牌（`plane_api_...`）。 |
| `PLANE_WORKSPACE` | 工作区 slug（不是实例名）。 |

配置文件按优先级从多个位置发现（取第一个存在的）：`~/.config/pbot/config.toml`（TOML，顶层 `base_url` / `api_key` / `workspace`，也可放在 `[auth]` 段）、`~/.pbot`、`~/.planecli`，最后是旧的 `~/.plane_api`（`key=value` 文本，键为小写，`chmod 600`）。优先级：命令行参数 > 环境变量 > 配置文件。`pbot configure` 仍写入 `~/.plane_api`。

> 自托管实例若在代理后面，请这样运行，让 CLI 请求直连实例：`env -u http_proxy -u https_proxy -u HTTP_PROXY -u HTTPS_PROXY -u all_proxy pbot ...`

## 退出码

| 码 | 含义 |
|---|---|
| 0 | 成功 |
| 1 | 通用错误 |
| 2 | 认证失败（HTTP 401）——以及**用法错误**（clap 拒绝的参数或子命令） |
| 3 | 资源不存在（HTTP 404） |
| 4 | API 错误——服务端回的其他状态（400、403、409、429、5xx）——以及传输失败 |
| 5 | CLI 自己抛出的校验错误（客户端侧） |

写脚本读退出码之前，有两点需要先知道：

- **用法错误也返回 2，和认证失败是同一个码。** 缺少必填参数、写了不存在的参数、子命令缺少
  位置参数，这些都由 clap 在发出任何请求之前拒绝；错误信息和用法说明都写到 **stderr**，所以
  stdout 仍然是干净的（`--json 2>/dev/null` 得到的是空输出，而不是半截 JSON）。因此码 2 表示
  「CLI 或凭证没能走到那一步」，它本身**不区分**密码写错还是参数写错。
- **权限不足在大多数命令上返回 4，在测试用例功能上返回 2。** 通用映射把服务端回的、既不是 401
  也不是 404 的状态一律归到 API 类，所以 403（服务主体在该项目上没有授权）返回 4。测试用例功能
  （`tc`、`run`）有自己的映射，把类别分得更细：403 是**权限失败**（返回 2，报错里带着服务端
  自己的补救说明）；带**稳定原因码**的 400 或 409（`invalid_test_case`、`conclusion_is_final`、
  `evidence_missing` 等）是**校验失败**（返回 5）——调用方可以据此行动。不带原因码的 400 或 409
  在那里同样返回 5，因为这个 API 的冲突是对写入的拒绝，不是传输故障。

测试用例功能（`tc`、`run`）的错误分类汇总：

| 类别 | 退出码 | 例子 |
|---|---|---|
| 成功 | 0 | 写入成功、读取成功 |
| 认证 / 权限 | 2 | `401`、`403`——服务主体在该项目上没有授权 |
| 用法错误 | 2 | 缺少必填参数、未知参数 |
| 资源不存在 | 3 | 项目里没有这个用例或这个批次 |
| API / 传输 | 4 | 5xx、连接失败、客户端读不懂的应答 |
| 校验 | 5 | 本地检查，以及服务端带稳定原因码的每一次拒绝 |

## 命令索引

| 命令 | 用途 |
|---|---|
| [`whoami`](#pbot-whoami) | 查看当前登录用户。 |
| [`configure`](#pbot-configure) | 交互式写入凭证（到生效的配置文件）。 |
| [`user`](#pbot-user) | 工作区成员。 |
| [`cache`](#pbot-cache) | 本地磁盘缓存管理。 |
| [`project`](#pbot-project) | 项目。 |
| [`wi`](#pbot-wi) | 工作项（别名：`work-item`、`issues`、`issue`）。 |
| [`relations`](#pbot-relations) | 工作项关系（别名：`relation`）。 |
| [`comment`](#pbot-comment) | 工作项评论。 |
| [`label`](#pbot-label) | 标签。 |
| [`state`](#pbot-state) | 状态。 |
| [`module`](#pbot-module) | 模块。 |
| [`cycle`](#pbot-cycle) | 周期（Sprint）。 |
| [`intake`](#pbot-intake) | 收件箱队列。 |
| [`doc`](#pbot-doc) | 文档 / 页面。 |
| [`attachment`](#pbot-attachment) | 工作项附件（别名：`attachments`）。 |
| [`tc`](#pbot-tc) | 项目的测试用例。 |
| [`run`](#pbot-run) | 项目的测试批次。 |

---

## pbot whoami

查看当前登录用户。

```
pbot whoami [--json]
```

`--json` 输出 `/api/v1/users/me/` 的原始响应。表格的 `name` 行会标明凭证类型：**人类账户**直接显示名字；**服务主体**显示 `<名称> (service principal)`（机器人 token，没有可显示的人名字段），若响应里带 owner 名则显示 `<名称> (SP owned by <owner>)`。

**为什么通常看不到 owner。** `/api/v1/users/me/` 刻意不序列化 owner 的资料：服务主体不允许读取拥有它的人类账户 —— 这是服务端的**权限设计**，不是客户端的能力缺失。因此 owner 采用前向兼容读取：只有当响应里出现 `owner`（对象或名字）或 `owner_name` 字段时才会显示上面的标记。会话专用的 `/api/workspaces/<slug>/service-principals/` 接口确实返回 `owner`，但它拒绝服务主体 token（403），所以 CLI 不会去探测它。

### 服务主体下的 `me`

`me` 解析为当前凭证的**影子机器人成员**：Plane 用来记录该主体写入的 `User` 行，`pbot user ls` 中显示为 `svc-<名称> (bot)`。这点很关键：服务主体调用 `/users/me/` 得到的是 `ServicePrincipal.id`，它**不是**成员 id —— 用它指派不会生效，用它过滤只会得到空列表。`me` 不会把这个 id 发给 API。

服务主体下的每个 `me` 结果都会说明解析到了什么，方便无人值守的调用方无需读源码即可判断：

- 任何以 `--assignee me` 发起的写入（`wi assign`、`wi create`、`wi update`）在写入落地后都会打印
  `Work item assigned to <名称> (service principal) as its shadow bot member '<成员行名称>' (member id <uuid>).`
  其中 `<成员行名称>` 取匹配到的成员行自身的 `display_name`（即 `pbot user ls --json` 里的该字段；人读表格对带该字段的行也显示它），便于据此定位该成员。
- `wi ls --assignee me` 在没有匹配行时打印一行 note：说明主体、影子 bot 成员及其 UUID，以及下一条应执行的命令（`pbot wi ls -p <project> --assignee '<成员名或 uuid>'`，成员列表用 `pbot user ls`）。
- 找不到影子 bot 成员时，`me` 报错（退出码 3），hint 指向 `pbot user ls`，而不是返回一个会被静默忽略的 id。
- 如果成员列表本身读不到：`wi ls --assignee` 用名字或 `me` 时会以 hint 报错（退出码 3，没有成员列表无法完成过滤，而丢掉过滤会返回调用方没要的行）；显式给出的成员 id 仍可正常过滤 —— 那种情况下成员列表只用于显示名字。
- 普通列表两种情况下都照常运行 —— 成员列表对它只是富化信息：负责人一列回退为成员 id 的前 8 个字符，并有一行 warning 指明是哪个调用被拒（`warning: could not read the workspace member list (…); assignee names will show the member id's first 8 characters — check access with 'pbot user ls'`）。成员列表只在真正会被用到时才读取：不带 `--assignee`、且所有行的负责人为空的列表不会发起该请求，也不会打印 warning。

人类凭证的 `me` 输出保持不变：解析到账号自身的成员 id；显式给出的名字或 UUID 也不会打印任何身份 note。

## pbot configure

交互式写入凭证（依次提示实例地址、API Key、工作区 slug），保存到生效的配置文件（优先级最高的已存在候选，默认 `~/.plane_api`），随后清空磁盘缓存。

```
pbot configure
```

## pbot user

### pbot user list

列出工作区成员。

**别名** `ls`

```
pbot user list [--json]
```

## pbot cache

### pbot cache clear

清空本地磁盘缓存。

```
pbot cache clear
```

---

## pbot project

### pbot project list

列出工作区内的项目。

**别名** `ls`

```
pbot project list [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--state <state>` | 按项目状态过滤。 |
| `--sort <field>` | 排序字段：`created` 或 `linear`（默认 `linear`）。 |
| `-l, --limit <n>` | 最大条数（默认 50）。 |

### pbot project show

查看项目详情。

```
pbot project show <project> [--json]
```

### pbot project create

新建项目。

```
pbot project create <name> [-i <identifier>] [-d <description>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-i, --identifier <id>` | 项目标识符（省略时按名称自动生成）。 |
| `-d, --description <text>` | 项目描述。 |

### pbot project update

更新项目。

```
pbot project update <project> [--name <name>] [-i <identifier>] [-d <description>] [--json]
```

### pbot project delete

删除项目。

```
pbot project delete <project>
```

---

## pbot wi

工作项管理。**组别名：** `work-item`、`issues`、`issue`。

### pbot wi list

列出工作项。省略 `-p` 时跨所有项目列出。

```
pbot wi list [-p <project>] [--assignee <name|me>] [--state <s>] [--labels <a,b>]
             [--parent <id>] [--sort <field>] [-l <n>] [--json]
```

**别名** `ls`

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <name\|id>` | 项目名称、标识符或 UUID；省略则跨全部项目。 |
| `--assignee <name\|me>` | 按负责人过滤。 |
| `--state <s>` | 按状态名过滤（逗号分隔）。 |
| `--labels <a,b>` | 按标签名过滤（逗号分隔）。 |
| `--parent <id>` | 列出某父项的子工作项（`ABC-123`、UUID 或名称）。 |
| `--sort <field>` | `created`（默认）或 `updated`。 |
| `-l, --limit <n>` | 最大条数（默认 50）。 |

**示例**

```
$ pbot wi ls -p PLANECLI --state "In Progress" --assignee me --json
$ pbot wi ls -p PLANECLI --parent PLANECLI-38 --json
```

### pbot wi show

查看工作项详情。JSON 含 `web_url`、`parent` 与 `sub_issues`。

```
pbot wi show <issue> [-p <project>] [--no-comments] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <name\|id>` | 项目名称/ID（按名称查找时必填）。 |
| `--no-comments` | 跳过拉取评论。 |

### pbot wi create

新建工作项。

**别名** `new`

```
pbot wi create <title> [-p <project>] [--assign <name|me>] [--state <s>]
              [--labels <a,b>] [--priority <p>] [--parent <id>]
              [-d <text> | --desc-md <markdown>] [--start-date <date>]
              [--target-date <date>] [-i <file>...] [--force] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <name\|id>` | 项目（必填）。 |
| `--assignee, --assign <name\|me>` | 负责人。 |
| `--state <s>` | 状态名（写入前按项目校验）。 |
| `--labels <a,b>` | 逗号分隔的标签名（写入前按项目校验）。 |
| `--priority <p>` | `urgent`、`high`、`medium`、`low`、`none`（或 `1`–`4`、`0`）。 |
| `--parent <id>` | 父工作项（`ABC-123`、UUID 或名称）。 |
| `-d, --description <text>` | 描述（纯文本，包一层段落）。 |
| `--desc-md <markdown>` | 描述（原生 markdown：标题/列表/代码/链接）。与 `-d` 互斥。 |
| `--start-date <YYYY-MM-DD>` | 开始日期（格式校验，写入后回读确认）。 |
| `--target-date <YYYY-MM-DD>` | 截止日期。 |
| `-i, --image <file>` | 上传图片并嵌入描述（可重复）。 |
| `--force` | 同名附件已存在时仍上传图片。 |

**示例**

```
$ pbot wi create "修复登录" -p PLANECLI --assign me --state Todo --priority high --json
$ pbot wi create "子任务" -p PLANECLI --parent PLANECLI-9 --desc-md "# 背景

- 要点" --json
```

### pbot wi update

更新工作项。日期与父项的写入会回读校验。

```
pbot wi update <issue> [-p <project>] [--state <s>] [--priority <p>]
              [--assign <name|me>] [--labels <a,b>] [--clear-labels]
              [--name <title>] [-d <text> | --desc-md <markdown>]
              [--start-date <date>] [--target-date <date>]
              [--parent <id>] [--clear-parent] [-i <file>...] [--force] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <name\|id>` | 项目（按名称查找时需要）。 |
| `--state <s>` | 新状态（校验，缺失时列出可用项）。 |
| `--priority <p>` | 新优先级。 |
| `--assignee, --assign <name\|me>` | 新负责人。 |
| `--labels <a,b>` | 设置标签（逗号分隔）。 |
| `--clear-labels` | 清空全部标签。 |
| `--name <title>` | 新标题。 |
| `-d, --description <text>` | 新描述（纯文本）。 |
| `--desc-md <markdown>` | 新描述（markdown）。与 `-d` 互斥。 |
| `--start-date <date>` / `--target-date <date>` | 新日期。 |
| `--parent <id>` | 设置父项（自引用/跨项目/不存在会被本地拒绝）。 |
| `--clear-parent` | 解除父项。 |
| `-i, --image <file>` | 上传图片并追加到描述（可重复）。 |
| `--force` | 允许同名图片重复上传。 |

**示例**

```
$ pbot wi update PLANECLI-40 --state "In Progress" --json
$ pbot wi update PLANECLI-40 --parent PLANECLI-38 --json
$ pbot wi update PLANECLI-40 --clear-parent --json
```

### pbot wi delete

删除工作项。

```
pbot wi delete <issue> [-p <project>]
```

### pbot wi search

按文本搜索工作项。

```
pbot wi search <query> [-p <project>] [-l <n>] [--json]
```

### pbot wi assign

指派工作项（默认指派给自己）。

```
pbot wi assign <issue> [--assign <name|me>] [-p <project>]
```

**另见** `pbot relations`、`pbot comment`

---

## pbot relations

工作项关系管理。**组别名：** `relation`。

关系类型：`blocking`、`blocked_by`、`duplicate`、`relates_to`、`start_before`、`start_after`、`finish_before`、`finish_after`。

### pbot relations list

列出某工作项的关系（八个分类桶）。

**别名** `ls`

```
pbot relations list <issue> [-p <project>] [--json]
```

### pbot relations add

为某工作项建立一条或多条关系。

```
pbot relations add <issue> --type <type> --to <target>... [-p <project>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--type <type>` | 八种关系类型之一。 |
| `-t, --to <target>` | 目标工作项（`ABC-123`、UUID 或名称）；可重复。 |

**示例**

```
$ pbot relations add PLANECLI-38 --type relates_to --to PLANECLI-40 -p PLANECLI --json
```

### pbot relations remove

**暂不可用** —— 后端缺少关系删除接口。该命令会以校验错误退出，并指向对应的后端需求。

---

## pbot comment

### pbot comment list

列出某工作项的评论（从旧到新）。

**别名** `ls`

```
pbot comment list <issue> [-p <project>] [-l <n>] [--json]
```

### pbot comment create

新增评论。

**别名** `new`

```
pbot comment create <issue> (-b <text> | --body-md <markdown> | --body-file <PATH> | --body-md-file <PATH>) [-p <project>] [--json]
```

四个正文 flag 必须且只能传一个；同时传两个会在 clap 解析阶段直接拒绝（不发送请求）。
`--body-file` / `--body-md-file` 接受路径字面量（`./-` 仍是名为 `-` 的文件）；传 `-` 时从
stdin 读入（例如 `pbot wi show ABC-1 | pbot comment create ABC-2 --body -`）。正文超过内核
单参数上限（约 130 KB）时请走文件 / stdin 这两条路径。

**选项**

| 选项 | 说明 |
|---|---|
| `-b, --body <text>` | 评论正文（纯文本；分段、`code`、裸链接会转成 HTML）。 |
| `--body-md <markdown>` | 评论正文（原生 markdown）。与其他正文 flag 互斥。 |
| `--body-file <PATH>` | 从 UTF-8 文件读取纯文本正文（`-` 读 stdin）。等价于 `--body "$(cat PATH)"`。与其他正文 flag 互斥。 |
| `--body-md-file <PATH>` | 从 UTF-8 文件读取 markdown 正文（`-` 读 stdin）。等价于 `--body-md "$(cat PATH)"`。与其他正文 flag 互斥。 |

### pbot comment update

更新评论。

```
pbot comment update <comment-id> --issue <issue> (-b <text> | --body-md <markdown> | --body-file <PATH> | --body-md-file <PATH>) [--json]
```

四个正文 flag 互斥（见 `comment create`）。

### pbot comment delete

删除评论。

```
pbot comment delete <comment-id> --issue <issue> [-p <project>]
```

---

## pbot label

### pbot label list

列出项目中的标签。

**别名** `ls`

```
pbot label list -p <project> [--json]
```

### pbot label show

查看标签详情。

```
pbot label show <label> -p <project> [--json]
```

### pbot label create

新建标签。

```
pbot label create <name> -p <project> [--color <#RRGGBB>] [--json]
```

### pbot label update

更新标签。

```
pbot label update <label> -p <project> [--name <name>] [--color <#RRGGBB>] [--json]
```

### pbot label delete

删除标签。

```
pbot label delete <label> -p <project>
```

---

## pbot state

### pbot state list

列出项目中的状态。

**别名** `ls`

```
pbot state list -p <project> [--group <group>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--group <group>` | 按分组过滤：`backlog`、`unstarted`、`started`、`completed`、`cancelled`。 |

### pbot state show

查看状态详情。

```
pbot state show <state> -p <project> [--json]
```

### pbot state create

新建状态。

```
pbot state create <name> -p <project> [--group <group>] [--color <#RRGGBB>] [--json]
```

### pbot state update

更新状态。

```
pbot state update <state> -p <project> [--name <name>] [--group <group>] [--color <#RRGGBB>] [--json]
```

### pbot state delete

删除状态。

```
pbot state delete <state> -p <project>
```

---

## pbot module

### pbot module list

列出项目中的模块。

**别名** `ls`

```
pbot module list -p <project> [--json]
```

### pbot module show

查看模块详情。

```
pbot module show <module> -p <project> [--json]
```

### pbot module create

新建模块。

```
pbot module create <name> -p <project> [-d <text>] [--start-date <date>]
                    [--end-date <date>] [--status <status>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--status <status>` | `backlog`、`planned`、`in-progress`、`paused`、`completed`、`cancelled`。 |

### pbot module update

更新模块。

```
pbot module update <module> -p <project> [--name <name>] [-d <text>] [--status <status>]
                    [--start-date <date>] [--end-date <date>] [--json]
```

### pbot module delete

删除模块。

```
pbot module delete <module> -p <project>
```

---

## pbot cycle

### pbot cycle list

列出项目中的周期。

**别名** `ls`

```
pbot cycle list -p <project> [--json]
```

### pbot cycle show

查看周期详情。

```
pbot cycle show <cycle> -p <project> [--json]
```

### pbot cycle create

新建周期。

```
pbot cycle create <name> -p <project> [-d <text>] [--start-date <date>] [--end-date <date>] [--json]
```

### pbot cycle update

更新周期。

```
pbot cycle update <cycle> -p <project> [--name <name>] [--start-date <date>] [--end-date <date>] [--json]
```

### pbot cycle delete

删除周期。

```
pbot cycle delete <cycle> -p <project>
```

### pbot cycle add-item

把工作项加入周期。

```
pbot cycle add-item <cycle> <issue> -p <project>
```

### pbot cycle remove-item

把工作项移出周期。

```
pbot cycle remove-item <cycle> <issue> -p <project>
```

### pbot cycle items

列出周期内的工作项。

```
pbot cycle items <cycle> -p <project> [--json]
```

---

## pbot intake

### pbot intake list

列出项目的收件箱队列。每行含队列包装 `id` 与工作项 `issue_id`。

**别名** `ls`

```
pbot intake list -p <project> [--json]
```

### pbot intake create

新建收件箱条目。

**别名** `new`

```
pbot intake create <name> -p <project> [-d <text>] [-P <priority>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-d, --description <text>` | 描述（会做 HTML 转义，标签按文本显示）。 |
| `-P, --priority <priority>` | 优先级（`none`、`low`、`medium`、`high`、`urgent`）。 |

### pbot intake accept

接受收件箱条目（triage）。需项目 Admin 角色；CLI 会校验状态是否真的变化，未生效则报错。

```
pbot intake accept <issue-id> -p <project> [--json]
```

### pbot intake decline

拒绝收件箱条目（triage）。需项目 Admin 角色。

```
pbot intake decline <issue-id> -p <project> [--json]
```

### pbot intake delete

删除收件箱条目。**破坏性：** 状态不是 `accepted` 时，会连同底层工作项一起删除。

```
pbot intake delete <issue-id> -p <project>
```

### pbot intake enabled

查看项目是否启用收件箱。

```
pbot intake enabled <project> [--json]
```

---

## pbot doc

所有 `doc` 子命令都必须带 `-p <project>`：v1 API 只提供项目级页面路由，不存在工作区级页面路由。`list` / `create` / `update` / `archive` / `delete` 缺 `-p` 时在发请求前就被本地拒绝——退出码 5，并给出点名 `-p <project>` 的 `Hint:`。`show` 是唯一的例外：它会先尝试查询，再把 404 换成同一条提示，这样只存在于其他作用域的页面 UUID 也能得到明确答复。`--dry-run` 同样适用：`create` / `update` 的预览也要求 `-p`，缺了同样被拒；此前它们不带 `-p` 也会打印计划。

### pbot doc list

列出文档。必须带 `-p`（项目内页面）。

**别名** `ls`

```
pbot doc list -p <project> [--json]
```

### pbot doc show

查看文档详情。

**别名** `read`

```
pbot doc show <doc> [-p <project>] [--json]
```

`-p <project>` 实际上必填：v1 API 只提供项目级页面路由，省略 `-p` 会命中一个服务端并不存在的工作区级路由。此时命令会用原 404 文案加一条提示告诉下一步该传 `-p`，例如 `Error: Resource not found (HTTP 404).` 之后接 ``Hint: This server has no workspace-level pages route; pass `-p <project>` to look up a project page.``（退出码 3）。带 `-p` 时查找、成功输出和退出码与之前一致。

### pbot doc create

新建文档。

**别名** `new`

```
pbot doc create --title <title> -p <project>
                (--content <markdown> | --content-md <markdown> | --content-raw <text> | --content-html <html>)
                [--dry-run] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--title <title>` | 页面标题（必填）。 |
| `-c, --content <markdown>` | 内容（Markdown：标题 / 列表 / 代码 / 加粗斜体 / 链接 / 图片，转 HTML）。与 `--content-md` 等价。**v1.4.0 起 BREAKING**：`# 标题` 现在被渲染为 `<h1>标题</h1>` 而不是字面文本；想要旧版纯文本行为请用 `--content-raw`。 |
| `--content-md <markdown>` | 与 `--content` 等价（同一条 markdown → HTML 路径）。 |
| `--content-raw <text>` | 内容（纯文本：空行分段，单换行变 `<br/>`；`#`、`-`、`*`、`[ ]` 等 markdown 字符当字面）。即 v1.3.x 的 `--content` 行为。 |
| `--content-html <html>` | 内容（原样 HTML，存原文，适合富排版）。 |
| `-p, --project <name\|id>` | 项目（必填；省略会在本地被拒绝，退出码 5）。 |
| `--dry-run` | 只报告目标与每张图片的判定，不做任何写入；图片缺失或 MIME 不允许时非零退出。解析不了的引用会列在 `unprocessed` 下并计入汇总行，但不改变退出码。 |

四种内容输入互斥。

**`--content` / `--content-md` 里的图片无需额外参数**：本地路径（`./a.png` 或 `file://…`）会上传为页面资产并把 src 替换为 asset id；远端 URL 或已是 asset id 的原样保留；围栏代码块里的图片不处理。文件缺失/不可读、或 MIME 不在
`image/jpeg|png|webp|gif` 内则命令失败（SVG 服务端拒绝）。

`src` 必须是**不含空格的单个 token**（这正是 markdown 转换器会变成 `<img>` 的形状），
所以含空格的本地路径要用尖括号包住：`![alt](<./a b.png>)`；`%XX` 转义会被解码
（`file:///tmp/a%20b.png` 能找到 `a b.png`）。其余「看起来像图片但处理不了」的写法 ——
没加尖括号的空格、`"title"` 后缀、空 `src`、尖括号里不是本地路径 —— **不算错误**：
该引用会以字面 markdown 留在页面上，同时 pbot 在 stderr 给出告警。`--dry-run` 会把它们
列在 `unprocessed` 下并计入汇总行，避免报告读起来像「一切正常」。

告警与报告是**给人读的，不是机器契约**：告警形如
`warning: line N: … was not uploaded — <原因>; it will stay literal markdown on the page`，
汇总行为 `summary: N to upload, N kept, N error(s), N unprocessed`。`--json` 返回的是页面对象，
两者都不包含 —— 脚本要判断「有引用未被处理」，请解析 `--dry-run` 的汇总行。
`doc update --dry-run` 使用同一套图片文法与报告。

**示例**

```
$ pbot doc create --title "Runbook" -p PLANECLI --content-md "$(cat runbook.md)" --json
$ pbot doc create --title "Plain note" -p PLANECLI --content-raw "see # heading" --json
$ pbot doc create --title "Spec" -p PLANECLI --content-html "$(cat spec.html)" --json
$ pbot doc create --title "Spec" -p PLANECLI --content-md "$(cat spec.md)" --dry-run
```

### pbot doc update

更新文档。

```
pbot doc update <doc> -p <project> [--title <title>]
                     [--content <markdown> | --content-md <markdown> | --content-raw <text> | --content-html <html>]
                     [--dry-run] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--title <title>` | 新的页面标题。 |
| `-c, --content <markdown>` | 新内容（Markdown，与 `--content-md` 等价）。同 `doc create --content` 的 BREAKING 行为。 |
| `--content-md <markdown>` | 新内容（Markdown；图片处理同 `doc create`）。 |
| `--content-raw <text>` | 新内容（纯文本，v1.3.x 的 `--content` 行为）。 |
| `--content-html <html>` | 新内容（原样 HTML）。 |
| `-p, --project <name\|id>` | 项目（必填；省略会在本地被拒绝，退出码 5）。 |
| `--dry-run` | 只报告目标与每张图片判定，不做任何写入；图片不可处理时退出码 5。解析不了的引用会列在 `unprocessed` 下并计入汇总行，但不改变退出码。 |

### pbot doc archive

归档（进回收站）页面，不删除；页面在 Web 回收站可恢复。

```
pbot doc archive <doc> -p <project>
```

### pbot doc delete

删除页面。API 只允许删除已归档页面，因此本命令先归档（并回读确认归档生效）再删除。

```
pbot doc delete <doc> -p <project>
```

---

## pbot attachment

**组别名：** `attachments`。

### pbot attachment attach

上传附件到工作项（预签名三步上传，写后回读验证）。

**别名** `upload`、`new`

```
pbot attachment attach <issue> -f <file> [-p <project>] [--force] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-f, --file <path>` | 待上传文件。 |
| `--force` | 同名附件已存在时仍上传。 |

### pbot attachment list

列出工作项的附件。

**别名** `ls`

```
pbot attachment list <issue> [-p <project>] [--json]
```

---

## pbot tc

项目的**测试用例**。用例由编号命名（`TC-<模块码>-<NNN>`），编号**由服务端分配**——CLI 从不自己
拼编号。模块码是编号的前缀，所以要先注册（`pbot tc module create`）；服务端拒绝模块未在项目
中注册的用例。

一条用例有**六个必填的头部字段**——标题、目的、被测对象、优先级、前置条件、环境——以及**至少
一步**。CLI 同样要求这六项，所以它的 `--help` 说的是服务端真正会拒绝的东西。

每个读命令的 `--json` 走 stdout、表格走 stderr；每个写命令打印服务端分配的编号。被拒绝的写入
会打印服务端给出的**稳定原因码**（`invalid_test_case`、`module_code_format`、`module_code_taken`、
`expected_is_unjudgeable` 等），并附上服务端的那句话，每个问题一行，各自标出它属于哪条用例、
哪一步。**请按原因码判断，不要按中文散文判断**——原因码是错误第一行的第一个词，也是服务端承诺
会保持不变的那部分。

### pbot tc list

列出项目的测试用例。

**别名** `ls`

```
pbot tc list -p <project> [--module <code>] [--priority <P>] [--issue <工作项>]
                  [--status <active|all>] [--search <文本>] [-l <limit>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <project>` | 项目名称、标识符或 UUID（必填）。 |
| `--module <module>` | 按模块码或模块 UUID 过滤。 |
| `--priority <priority>` | 按优先级过滤：`P0`、`P1`、`P2`、`P3`。 |
| `--issue <issue>` | 按已关联的工作项过滤（标识符或 UUID）。 |
| `--status <status>` | 按状态过滤：`active`（服务端默认）或 `all`。 |
| `--search <text>` | 按编号或标题过滤。 |
| `-l, --limit <limit>` | 最多显示多少条（默认 50）。 |

### pbot tc show

显示一条用例：头部字段、步骤、工作项关联。

```
pbot tc show <case> -p <project> [--json]
```

`<case>` 可以是编号（`TC-CLI-001`）、UUID 或标题。

### pbot tc create

创建测试用例。编号由服务端分配。

```
pbot tc create -p <project> --module <code> --title <title> --purpose <purpose>
                  --object-overview <text> --priority <P> --precondition <text>
                  --environment <text> [--related-feature <text>] [--remarks <text>]
                  [--status <active|void>] [--step "操作 | 预期结果"]...
                  [--steps-file <path>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <project>` | 项目名称、标识符或 UUID（必填）。 |
| `--module <module>` | 用例所属的模块码（编号的前缀）。 |
| `--title <title>` | 用例标题。**服务端必填。** |
| `--purpose <purpose>` | 这条用例验证什么。**服务端必填。** |
| `--object-overview <text>` | 被测对象是什么。**服务端必填。** |
| `--priority <priority>` | `P0`、`P1`、`P2`、`P3`。**服务端必填。** |
| `--precondition <text>` | 执行前必须成立的条件。**服务端必填。** 别名 `--preconditions`。 |
| `--environment <text>` | 环境与被测版本。**服务端必填。** 别名 `--env`。 |
| `--related-feature <text>` | 用例所属的功能。 |
| `--remarks <text>` | 自由备注。 |
| `--status <status>` | `active`（默认）或 `void`。 |
| `--step "操作 \| 预期结果"` | 一步（可重复）。 |
| `--steps-file <path>` | 存放整张步骤表的文件。 |

**步骤的格式是 `操作 | 预期结果`**——操作、竖线、预期结果，写在一行里。两半都必须有；没有竖线
或有一半是空的行，会在发出任何请求之前就被本地拒绝（退出码 5）。

`--steps-file` 用两种形态之一给出整张步骤表：

- **JSON 数组**：`[{"action": "打开页面", "expected": "列表已显示"}]`（第二个键也接受
  `expected_result`）；
- **纯文本行**：每行一个 `操作 | 预期结果`。空行和以 `#` 开头的行会被跳过。

两种形态喂给同一张表。表是**整张**表：创建时写入给它的每一步，之后带步骤的 `tc steps` 会把表
整体替换（见下）。

**示例**

```bash
pbot tc create -p PLANECLI --module CLI \
  --title "用有效账号登录" \
  --purpose "检查 Web UI 的登录路径" \
  --object-overview "Web UI 的登录表单" \
  --priority P1 \
  --precondition "账号存在且处于启用状态" \
  --environment "Windows 10、Chrome 120" \
  --step "打开登录页 | 表单已显示" \
  --step "用有效账号登录 | 控制台已显示" \
  --json
```

### pbot tc update

更新测试用例。所有字段可选；没给的字段保持原样。

```
pbot tc update <case> -p <project> [--title <title>] [--purpose <purpose>]
                  [--object-overview <text>] [--priority <P>] [--precondition <text>]
                  [--environment <text>] [--related-feature <text>] [--remarks <text>]
                  [--status <active|void>] [--step "操作 | 预期结果"]...
                  [--steps-file <path>] [--json]
```

**用例的编号和模块都不能改。** 编号绑定在模块上，并且是每一条执行记录的组成部分，服务端两者
都保留。`--module` 与 `--number` 这两个参数存在，只是为了让拒绝信息能点名调用方想改的那个
字段；两者都会在发出任何请求之前被本地拒绝（退出码 5），所以不需要回滚任何东西。

`--step` / `--steps-file` 替换的是**整张**步骤表：每一步都要重新传一遍，服务端会丢掉不在
列表里的那些行。

### pbot tc steps

读取或替换一条用例的步骤表。

```
pbot tc steps <case> -p <project> [--step "操作 | 预期结果"]...
                  [--steps-file <path>] [--json]
```

不带步骤就是读表；带步骤就**整体替换**这张表。

> **打印的是步骤表，不是用例编号。** `pbot tc steps TC-CLI-001` 打印的是表里的行——stderr 上是
> `Step`、`Action`、`Expected result` 三列的表格，stdout 上是同样内容的 JSON 数组——**而不是**
> `TC-CLI-001` 这个编号。用例编号由 `pbot tc show` 打印，步骤表由 `pbot tc steps` 打印。

### pbot tc issues

列出、增加或删除一条用例的工作项关联。

```
pbot tc issues <case> -p <project> [-a <工作项>] [-r <工作项>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-a, --add <work item>` | 关联一个工作项（标识符或 UUID）。 |
| `-r, --remove <work item>` | 解除一个工作项的关联（标识符或 UUID）。 |

`--add` 与 `--remove` 互斥；两个都不给就是列出关联。

### pbot tc delete

删除一条测试用例。**破坏性操作**：服务端只允许项目管理员删除用例，拒绝时带服务端自己的句子。

```
pbot tc delete <case> -p <project> [--json]
```

### pbot tc module list

列出项目已注册的模块码。

**别名** `ls`

```
pbot tc module list -p <project> [--json]
```

### pbot tc module create

注册一个模块码。模块码的规则归服务端管：2 到 6 个大写字母，项目内唯一。被拒绝的模块码会带上
服务端自己的原因码（`module_code_format`、`module_code_taken`）和它的句子。

```
pbot tc module create -p <project> --code <code> [-d <description>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <project>` | 项目名称、标识符或 UUID（必填）。 |
| `--code <code>` | 模块码，例如 `CLI`。 |
| `-d, --description <text>` | 这个模块覆盖什么。 |

---

## pbot run

项目的**测试批次（batch）**。一个批次是一次连续的用例执行。批次由编号命名
（`RUN-<YYYYMMDD>-<NN>`），编号**由服务端分配**——CLI 从不自己拼，也从不写编号。

这个功能的两条规则决定了整组命令的形状：

- **步骤判定**（步骤的 ✔ / ✘）和**用例结论**（用例的判定）都**只写一次**。服务端对第二次写入
  回 `conclusion_is_final`，写错了的做法是作废该记录、在新批次里重跑。所以这两个写命令都会先
  问一句，而且**本组没有任何参数能覆盖已经记录的判定**。
- **已完成**的批次被冻结：对它的每一次写入都会被 `run_completed` 拒绝，CLI 也不会自己重试这种
  拒绝。

### pbot run list

列出项目的批次。

**别名** `ls`

```
pbot run list -p <project> [--status <status>] [--run-date <YYYY-MM-DD>] [-l <limit>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <project>` | 项目名称、标识符或 UUID（必填）。 |
| `--status <status>` | 按状态过滤：`draft`、`in_progress`、`completed` 或 `all`。 |
| `--run-date <date>` | 按执行日期过滤（`YYYY-MM-DD`）。 |
| `-l, --limit <limit>` | 最多显示多少条（默认 50）。 |

### pbot run show

显示一个批次：头部、它里面的用例、这些用例的步骤和文件。

```
pbot run show <run> -p <project> [--json]
```

`<run>` 可以是编号（`RUN-20261109-01`）或 UUID。

### pbot run create

创建批次。编号由服务端分配。

```
pbot run create -p <project> --version <version> --build <build> --commit <commit>
                  --environment <env> [--run-date <YYYY-MM-DD>] [--retest-of <run>]
                  [--executed-by <who>] [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `-p, --project <project>` | 项目名称、标识符或 UUID（必填）。 |
| `--version <version>` | 被测版本（批次创建后冻结）。 |
| `--build <build>` | 被测构建号（批次创建后冻结）。 |
| `--commit <commit>` | 被测提交号（批次创建后冻结）。 |
| `--environment <env>` | 批次执行所在的环境。别名 `--env`。 |
| `--run-date <YYYY-MM-DD>` | 执行日期（默认：**UTC** 的今天）。 |
| `--retest-of <run>` | 本批次重测的那个批次（编号或 UUID）。 |
| `--executed-by <who>` | 谁执行了这个批次：`me`（默认）、姓名或 UUID。 |

之所以要 `--version`、`--build`、`--commit`、`--environment`，是因为服务端拒绝关闭一个不带这些
字段的批次，而本组没有命令可以事后补上。

**`--executed-by` 默认是 `me`**；在服务主体令牌下，`me` 解析到该主体的影子 bot 成员——和 CLI
其他地方用的是同一个 `me`。批次需要执行人：没有执行人的批次关不掉，服务端会用
`run_executed_by_missing` 拒绝 finalize。

**`--run-date` 是 UTC，而且创建之后就冻结。** 默认值是 *UTC 的今天*，不是操作者所在时区的今天：
UTC+8 的操作者在当地 00:00 到 08:00 之间创建批次，`run_date` 拿到的是**前一个 UTC 日**，批次
编号也带着那一天（`RUN-<YYYYMMDD>-<NN>`）。需要当地日期就显式传 `--run-date`。批次的日期之后
改不了——服务端会用 `run_date_fixed` 拒绝这次写入。

**`--retest-of` 把批次连到它重测的那个批次。** 重测只能在原批次所属的项目里进行，且不能成环：
服务端拒绝指向别的项目的链接（`retest_of_project_mismatch`）、指向自己的链接（`retest_of_self`），
以及会成环的链接（`retest_of_cycle`）。列表的 `Retest of` 列把这条链接显示为
`retest of RUN-20261109-01`——**英文**，与 Web UI 的「补测」不同。

### pbot run add

把用例加入批次。

```
pbot run add <run> <case>... -p <project> [--json]
```

服务端会为加入的每条用例**做一份快照**，所以之后修改用例不会改写批次记录下来的内容。已经在批次
里的用例会被 `duplicate_case_number` 拒绝；CLI 会先读批次，在发请求之前就拒绝重复项，并点名是
哪条用例。

### pbot run step

写入一步的判定与实测值。

```
pbot run step <run> --case <case> --step <n> --verdict <verdict>
                  [--actual <markdown>] [--actual-file <path>] [--image <path>]
                  [--yes] -p <project> [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--case <case>` | 批次里的那条用例（`TC-CLI-007`）。 |
| `--step <n>` | 用例里的第几步（1、2、3 …）。 |
| `--verdict <verdict>` | `pass`、`fail` 或 `unset`——没执行过的步骤是 `unset`。`N/A` 是用例的结论，不是步骤的判定，本地就会拒绝。 |
| `--actual <markdown>` | 实测值。 |
| `--actual-file <path>` | 存放实测值的文件。 |
| `--image <path>` | 步骤的截图。 |
| `-y, --yes` | 确认写入（命令不在终端上时必须给）。 |

**实测值原样到达服务端——不转换、不改写。** `--actual` 逐字发送这个字符串；`--actual-file`
逐字节发送文件内容，换行也照原样。两者都不在途中做 Markdown 转换：文件里是什么，服务端就存
什么。

**不在终端上时必须给 `--yes`。** 步骤判定只写一次，所以命令在写之前会问：终端上它在 stderr 上
提示（`… — write it? [y/N]`），不在终端上的脚本必须传 `--yes`。既没有 `--yes` 又没有终端时，
命令在发出任何请求之前就拒绝（退出码 5），什么都不会写。

**步骤的截图不满足 finalize。** 用例的证据挂在用例本身上——见下面的
[用例的证据规则](#用例的证据规则)。

### pbot run conclude

写入批次里一条用例的结论。

```
pbot run conclude <run> --case <case> --conclusion <conclusion>
                      [--evidence <markdown>] [--evidence-file <path>] [--image <path>]
                      [--remarks <text>] [--actual-result-summary <text>]
                      [--na-reason <text>] [--na-criteria-source <text>]
                      [--manual-executor <who>] [--manual-condition <text>]
                      [--yes] -p <project> [--json]
```

**选项**

| 选项 | 说明 |
|---|---|
| `--case <case>` | 批次里的那条用例（`TC-CLI-007`）。 |
| `--conclusion <conclusion>` | `PASS`、`FAIL`、`N/A` 或 `MANUAL`（`NA` 也接受，会归一成 `N/A`）。 |
| `--evidence <markdown>` | 用例的证据。 |
| `--evidence-file <path>` | 存放用例证据的文件（字节原样到达服务端）。 |
| `--image <path>` | 用例的截图——只有这一张截图就足够当证据。 |
| `--remarks <text>` | 自由备注。 |
| `--actual-result-summary <text>` | 用例的一句话现象（汇总会显示它）。 |
| `--na-reason <text>` | 用例为何不适用（`N/A` 的用例需要它）。 |
| `--na-criteria-source <text>` | `N/A` 用例的判定依据来自哪里。 |
| `--manual-executor <who>` | 谁手工执行的（`MANUAL` 的用例需要它）。 |
| `--manual-condition <text>` | 在什么条件下手工执行的。 |
| `-y, --yes` | 确认写入（命令不在终端上时必须给）。 |

用例结论**只写一次**：服务端对第二次写入回 `conclusion_is_final`，而且**没有任何参数能覆盖
它**。写错了的做法是作废该记录、在新批次里重跑（`pbot run create --retest-of <run>`）。

**拒绝时带哪个码，取决于顺序。** 在**未关闭**的批次上第二次写入，回的是 `conclusion_is_final`；
同样的写入放在 finalize **之后**，回的是 `run_completed`——已完成的批次被冻结，冻结先被检查。
请读码，不要按你预期的顺序推断。

### pbot run remove

把一条用例从批次里移出。

```
pbot run remove <run> --case <case> -p <project> [--json]
```

已经带判定或带结论的记录会留在批次里：服务端用 `run_case_removal_after_execution` 拒绝这次移出。

### pbot run finalize

关闭批次：服务端对它跑一遍有效性规则。

```
pbot run finalize <run> -p <project> [--json]
```

被拒绝的 finalize 会列出它找到的**每一个**问题，每个一行，各自标出属于哪条用例、哪一步：

```
Error: invalid_batch
conclusion_invalid (case TC-CLI-001): 结论必须是 PASS、FAIL、N/A 或需人工。
evidence_missing (case TC-CLI-001): 缺少必填项：证据。
Hint: Every reason above names the case and the step it belongs to; fix them and finalize again. …
```

批次保持打开，直到每个问题都被修好。关闭的批次被**冻结**——之后对它的每一次写入都会被
`run_completed` 拒绝。

### pbot run summary

批次的汇总：四个计数与偏差项。

```
pbot run summary <run> -p <project> [--json]
```

**这四个计数不是对总数的划分。** 一条用例在一步上失败、在另一条上从未执行，会被数两次，所以
`fail` 和 `manual` 可能数的是同一条用例：一个只有一条 `FAIL` 结论的批次会报 `total: 1`、
`fail: 1`、`manual: 1`。不要把计数加起来再跟 `total` 比。

---

## 用例的证据规则

服务端拒绝关闭一个用例没有证据的批次，而且它只数**用例级**的证据：一段非空的**用例级文本**
**或者**至少一个**用例级附件**。**步骤**上的截图挂在步骤记录上，所以**不算数**——finalize 会回
`evidence_missing`，路径是 `cases[TC-CLI-001].evidence`，并点名那条用例。

写用例证据有两种方式：`--evidence` / `--evidence-file`（文本）和 `--image`（文件），任何一种
单独就够：

```bash
# 文本
pbot run conclude RUN-20261109-01 --case TC-CLI-007 --conclusion FAIL --evidence "按钮一直是灰的" --yes
# 只有一张截图，不写文字
pbot run conclude RUN-20261109-01 --case TC-CLI-007 --conclusion FAIL --image ./shot.png --yes
```

CLI 读这段文本的方式和服务端一致——只有占位符的值（`—`、`TBD`、`待补充` 等）不算带了文本，
只有图片的值（`![](shot.png)`）也不算，因为图片是文件，不是句子。CLI 打印的提示**只是警告**：
它不会拒绝任何写入，最终仍然由服务端拒绝批次并点名用例。

### 残留：两种读法不一致的写法

本地预检查和服务端对某些 Markdown 写法的读法并不一致。这个差别**永远只是警告**——CLI 不会因此
拒绝写入，决定权始终在服务端——但知道这些写法的测试者，不会被一条服务端并不认可的警告吓到，
也不会对一条 CLI 没有提醒的拒绝感到意外。

**CLI 读成有文本、服务端读成没有文本**（CLI 不吭声，服务端拒绝 finalize）：占位符外面套着
块级标记——`> TBD`、`# TBD`、`- TBD`、`1. TBD`、`* TBD`；链接里的占位符——`[TBD](url)`；
`~~TBD~~`、`&nbsp;TBD`；带 info string 的围栏代码块——` ```text TBD ``` `；`~~~~` 围栏；以及链接
引用定义——`[ref]: shot.png`。

**CLI 读成没有文本、服务端读成有文本**（CLI 会警告，服务端接受）：未闭合的强调——`**TBD`、
`***TBD`、`TBD**`、`** ![](x.png) **`。

### 孤儿上传

`--image` 会**先上传并绑定文件，然后再发写入请求**，所以被服务端拒绝的写入可能留下一个已经绑在
记录上的附件——文件在记录上，判定不在。它**永远不会改变判定**：被拒绝的写入照样被拒绝，记录
保留它原来的判定。文件可以在下一次尝试时重传，结论不行。

---

## 另见

- `pbot <命令> <子命令> --help` —— 由 CLI 定义直接生成的逐命令帮助。
- Plane v1 API 参考：`docs/api/plane-v1-api.md`。
