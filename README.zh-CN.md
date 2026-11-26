[English](README.md) | [한국어](README.ko.md) | [日本語](README.ja.md) | **简体中文**

# bmo-drive

一个拥有独立 API 的单一服务形式的文件查看器。管理员可以把本机的磁盘、Garage 以及各节点的卷当作“位置”来浏览；
每个所有者都有一个带配额的、只属于自己的 BMO 网盘，可以读取和修改其中的内容；共享链接可以把一个文件或文件夹交给
没有令牌的人；成员共享则让读者所在公司的成员也能读取它。除服务之外，二进制文件还包含命令 `bmo-drive file share`、
客户端以及 harness 插件（MCP 服务器 `bmo-drive mcp` 和 `bmo-drive` 技能）。其他所有单元都通过各自的 API 访问，
详见 `docs/api.md`。

## 测试

测试平台（testbed）的 `personal-data` 检查负责执行个人数据规则（`PERSONAL-DATA.md`）。仓库中包含以下测试：
`tests/import.rs` 测试导入旧归档中的共享链接；`tests/owners.rs` 测试所有者及其两个分支：每个所有者的网盘只属于
自己，配额在字节到达之前就做出判断，机器只属于管理员，共享链接在两个分支中都能打开；`tests/members.rs` 测试成员
共享：与某个成员共享的文件或文件夹只有该成员能读取，共享结束后读取再次被拒绝；`tests/console.rs` 测试控制台
界面：用 bmo-ui 自己的读取方式、针对 `client/vendor/` 中的发布版本读取服务提供的清单（由 bmo-console-face 用 Bun
运行），确认界面连接到控制台的凭据，并且其他所有路由都由网盘自身守护；`tests/links.rs` 测试卷上的链接：指回自身的
路径会被拒绝，指向旁边文件夹的链接只遍历一次，每个链接的目标都会列出，文件夹的 zip 在链接处结束；
`tests/screen.rs` 测试 Files 页面，其模块同样用 Bun 运行：页面在哪里打开、位置记忆、共享给所有者的内容所在的
分支，以及每个共享都是通过成员自己的路径读取的只读位置。`cargo test` 会运行以上全部测试。前端包把浏览器测试放在
`client/tests/` 中：Files 页面在那里的一个小型 Nuxt 宿主中独立运行，由开发服务器连同前端包自己的文案一起提供，并在
Chromium 中针对服务的替身进行驱动。`tests/browser/files-upload.test.ts` 向打开的文件夹拖放和粘贴文件，粘贴一张由
浏览器在粘贴时命名的图片，向重命名输入框粘贴，并粘贴在页面上复制的条目；`tests/browser/files-view.test.ts` 在宽
窗口和手机宽度下通过工具栏选择视图；`tests/words/packs.test.ts` 确认语言包的四种语言区域拥有相同的键和占位符。前端包的检查，即 `client/` 中的 `bun run check`（锁定版本的安装、类型检查和
浏览器测试），与上述测试并列。`docs/inventory.md` 列出了旧行为所依据的测试。

## 目录结构

| 文件夹 | 内容 |
|---|---|
| `src/` | 服务、命令、客户端和 MCP 服务器，由一个 crate 拆分成各自的模块 |
| `migrations/` | 存储：按顺序创建各张表的语句 |
| `skills/` | `bmo-drive` 技能，与 MCP 服务器并列的 harness 插件的另一半 |
| `client/` | 控制台插件包：Files 页面和文件共享页面，是一个独立项目，构建配置位于 `client/console/` |
| `deploy/` | 服务的用户单元，以及部署发布的应用声明 |
| `docs/` | 服务行为的规范说明以及 OpenAPI 文档 |
| `scripts/` | 与声明一并生成的、把本应用从机器上移除的脚本 |
| `tests/` | 目前已有的行为测试 |

根目录下有 Cargo 清单、`bmo-app.toml`(唯一声明;`testbed.toml`、`install.toml`、`deploy/` 单元与 `scripts/remove.sh` 由它生成)、`rustfmt.toml`、`CLAUDE.md`、`PERSONAL-DATA.md`、
`korean-ledger.json`、`settings.toml`、`settings.local.toml.example`（部署时提供的本地设置示例）以及 `LICENSE`。

## 控制台插件包

`client/` 中是构建在 bmo-ui 之上的 Files 页面和文件共享页面。它是遵循 bmo-ui 控制台插件契约（bmo-ui 的
`docs/console-bundles.md`）的前端包，本服务在其中做出的选择见 `docs/api.md` 的「The console face」。它的文案放在
自己的语言包 `client/i18n/` 中，每个语言区域一个文件，位于 `drive.*` 下，并为 `docs/api.md` 中的每个拒绝代码提供
`drive.error.<code>`。

- **构建。** `bun run bundle` 借助 bmo-ui 的前端包构建，从页面源码生成 `client/dist/`。该构建不会打包控制台共享的
  库，并写出前端包所依赖的层：模块 `bundle.js`（其默认导出包含两个页面的路由）、样式表 `bundle.css`（页面在层的
  设计令牌上使用的工具类）、`words.json`（以语言区域为键的单个对象形式的语言包）以及 `layer.json`。页面未经导入就
  使用的组件和 Vue、Nuxt 函数会在构建时导入，构建配置位于 `client/console/`。每次发布时部署都会构建该文件夹，服务
  则连同清单一起提供这些文件。
- **层。** `client/vendor/` 中是 bmo-ui 发布命令原样写出的 bmo-ui 发布文件，也就是 bmo-chat 的
  `shared-versions.toml` 所固定的那一个；前端包针对它进行安装、构建和类型检查。组件库固定在该发布版本指定的确切
  版本，安装时会应用层自身的补丁（包括其基础组件和图标），补丁位于 `client/patches/`，每个补丁只适用于它所指定的
  确切版本。`client/` 中的 Nuxt 配置仅用于类型检查。
- **类型。** 页面对服务响应的类型由 `bun run api` 从 `docs/openapi.json` 生成到 `client/shared/openapi.ts`。

## 文档

| 文档 | 内容 |
|---|---|
| `docs/files.md` | 文件查看器：机器、位置、围栏、读取、修改、受保护位置、共享链接和成员共享 |
| `docs/api.md` | 路由、响应与拒绝、事件、命令、harness 插件、客户端、对其他单元的调用以及设置 |
| `docs/openapi.json` | 同一契约的机器可读形式，由 `bmo-drive openapi` 生成 |
| `docs/inventory.md` | 本仓库从旧仓库继承的内容，以及每一组的数量和来源 |
| `PERSONAL-DATA.md` | 本仓库中绝不能出现的内容（人的姓名、地址、路径、设备或网络信息、聊天室成员或公司的信息、真实对话中的任何一行），以及执行这一规则的检查 |

## 版本

版本属于整个套件，在构建时从 git 标签读取；crate 的版本只是永不变动的占位值（见 `docs/api.md` 的「Versions」）。

## 许可证

MIT。全文见 `LICENSE`。
