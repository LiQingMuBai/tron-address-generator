# TRON 靓号生成器

用 Rust 编写的 TRON（波场）靓号地址生成器。支持 **命令行**、**内嵌 Web 界面**、**桌面应用** 三种运行方式，全部计算在本地离线完成，不联网、不上传私钥。

## 特性

- **多线程狂暴搜索**：基于 `rayon` 并行生成私钥，实时显示尝试次数、keys/s 和命中进度
- **条件可自由组合**：连续重复字符、指定位置固定字符、自定义尾号、正则表达式，支持 `AND` / `OR`
- **XML 配置驱动**：`config.xml` 描述任务，命令行参数可临时覆盖
- **单文件 Web 界面**：HTML 通过 `include_str!` 编进二进制，运行不依赖任何外部文件
- **桌面应用**：原生窗口 + 系统 WebView（macOS WKWebView / Windows WebView2），双击即用
- **结果导出**：CSV / TXT / JSON
- **可选 Telegram 通知**：每命中一个地址推送一条消息，可关闭

## 地址派生流程

```
32 字节随机私钥
  → secp256k1 未压缩公钥（65 字节，0x04 开头）
  → Keccak-256
  → 取末 20 字节
  → 前置 0x41（TRON 网络前缀）
  → Base58Check 编码
  → 34 位、以 T 开头的地址
```

## 快速开始

```bash
# 编译
cargo build --release

# 命令行方式运行（默认读取 ./config.xml）
./target/release/tron-vanity

# 指定配置文件 / 覆盖参数
./target/release/tron-vanity --config my.xml --count 3 --threads 8
```

### 桌面版

```bash
cargo build --release --bin tron-vanity-desktop
./target/release/tron-vanity-desktop
```

桌面版会在后台起一个本地服务（随机端口），然后用系统 WebView 打开界面；同目录下的 `config.xml` 会被自动加载。

### Web 界面

```bash
./target/release/tron-vanity --web --port 8686
```

启动后自动打开浏览器，访问 `http://127.0.0.1:8686`。页面上可实时修改尾号、目标数量、线程数，查看进度、结果表和速率曲线，并直接下载导出文件。

## 命令行参数

| 参数 | 简写 | 默认值 | 说明 |
| --- | --- | --- | --- |
| `--config <FILE>` | `-c` | `config.xml` | XML 配置文件路径 |
| `--web` | | `false` | 启动内嵌 Web 界面，不走终端流程 |
| `--host <HOST>` | | `127.0.0.1` | Web 界面监听地址 |
| `--port <PORT>` | | `8686` | Web 界面监听端口 |
| `--count <N>` | `-n` | 配置文件值 | 覆盖 `TaskConfig/TargetCount` |
| `--threads <N>` | `-t` | 配置文件值 | 覆盖 `TaskConfig/ThreadCount`，`0` 表示自动取 CPU 核心数 |
| `--format <FMT>` | `-f` | 配置文件值 | 覆盖导出格式：`csv` / `txt` / `json` |
| `--output <DIR>` | `-o` | `.` | 导出文件输出目录 |
| `--no-progress` | | `false` | 关闭终端实时进度 |
| `--telegram-token <T>` | | 内置值 | 覆盖 Telegram bot token |
| `--telegram-chat <ID>` | | 内置值 | 覆盖接收通知的 chat id |
| `--no-telegram` | | `false` | 完全关闭 Telegram 通知 |

## 配置 `config.xml`

```xml
<?xml version="1.0" encoding="UTF-8"?>
<AddressGeneratorConfig>

    <!-- 链信息，TRON 只接受下面这组取值 -->
    <ChainConfig>
        <ChainType>TRON</ChainType>
        <AddressPrefix>T</AddressPrefix>
        <AddressLength>34</AddressLength>
        <PrivateKeyFormat>Hex</PrivateKeyFormat>
    </ChainConfig>

    <!-- 任务配置 -->
    <TaskConfig>
        <TargetCount>1</TargetCount>     <!-- 需要生成多少个符合条件的结果 -->
        <ThreadCount>0</ThreadCount>     <!-- 0 = 自动取 CPU 核心数 -->
        <ExportFormat>CSV</ExportFormat> <!-- CSV / TXT / JSON -->
    </TaskConfig>

    <!-- 条件组合 -->
    <CompositeConditions>
        <LogicOperator>AND</LogicOperator>  <!-- AND 全部满足 / OR 满足任一 -->

        <!-- 尾号连续 3 个 8 -->
        <Condition>
            <Type>Repeating</Type>
            <Param>
                <Char>8</Char>
                <MinCount>3</MinCount>
                <Position>Last</Position>   <!-- Last: 结尾 | Anywhere: 任意位置 -->
            </Param>
        </Condition>

        <!-- 倒数第 5 位固定为 A -->
        <Condition>
            <Type>FixedChar</Type>
            <Param>
                <Char>A</Char>
                <IndexFromEnd>5</IndexFromEnd>  <!-- 1 = 最后一位 -->
            </Param>
        </Condition>
    </CompositeConditions>

</AddressGeneratorConfig>
```

### 条件类型

| `Type` | `Param` 字段 | 说明 |
| --- | --- | --- |
| `Repeating` | `Char`、`MinCount`、`Position` | 要求出现连续 `MinCount` 个 `Char`。`Position` 取 `Last`（必须是结尾，默认）或 `Anywhere` |
| `FixedChar`（别名 `Fixed`） | `Char`、`IndexFromEnd` | 从末尾数第 `IndexFromEnd` 位必须是 `Char`（1 表示最后一位） |
| `Suffix`（别名 `EndsWith`、`Ends_With`） | `Value` | 地址以 `Value` 结尾 |
| `Regex` | `Pattern` | 地址需匹配该正则（`regex` crate 语义，部分匹配） |

未配置任何 `<Condition>` 时，每一个生成的地址都会被当作命中并立即返回。

### 配置校验规则

- `ChainType` 必须是 `TRON`，`PrivateKeyFormat` 必须是 `Hex`
- `AddressPrefix` 必须是 `T`，`AddressLength` 必须是 `34`
- `TargetCount` 必须大于 0

## 导出结果

文件名形如 `tron_vanity_20261119_064200.csv`，写在 `--output` 指定目录（默认当前目录）。

```csv
index,address,private_key,found_at_attempts
1,TYAcs9QRYgmQ4Z8oYVAtbHfcv2WAeAG888,9b7536b46a6084c552313e9abd3f7d7f029490029c7584c3e39c1f1d3b7d9eda,18607333
```

## Web API

Web 界面用到的接口（仅监听本机，无鉴权）：

| 方法 | 路径 | 说明 |
| --- | --- | --- |
| `GET` | `/` | 内嵌的 HTML 页面 |
| `GET` | `/api/defaults` | 读取 `config.xml` 的默认尾号 / 目标数 / 线程数 |
| `GET` | `/api/state` | 当前运行状态、进度、已生成结果 |
| `POST` | `/api/start` | 按传入参数开始搜索 |
| `POST` | `/api/stop` | 停止搜索 |
| `GET` | `/api/download` | 下载当前结果的导出文件 |

## 打包发布

`.github/workflows/build.yml` 提供 `Build desktop app` 工作流（Windows / macOS，仅打包桌面版），手动触发或推送 `v*` 标签即可运行：

| 平台 | 产物 |
| --- | --- |
| macOS Apple Silicon | `tron-vanity-macos.dmg`、`tron-vanity-macos-app.zip`（.app） |
| macOS Intel | 同上 |
| Windows x64 | `tron-vanity-windows-x64.zip`（绿色版）、`tron-vanity-windows-x64.msi`（安装包） |

Windows 的 `.msi` 由 [wix/installer.wxs](wix/installer.wxs) 生成，安装后含开始菜单和桌面快捷方式。

## 项目结构

```
src/
  lib.rs                      # 暴露给 CLI 与桌面版的公共模块
  main.rs                     # 命令行入口（bin: tron-vanity）
  bin/tron-vanity-desktop.rs  # 桌面版入口（bin: tron-vanity-desktop）
  config.rs                   # XML 配置解析与校验
  matcher.rs                  # 条件编译与匹配
  generator.rs                # 并行搜索、地址派生、观察者接口
  export.rs                   # CSV / TXT / JSON 导出
  notify.rs                   # Telegram 通知装饰器
  web.rs                      # axum 服务与 JSON API
  web_ui.html                 # 内嵌的单页前端
  error.rs                    # 统一错误类型
wix/installer.wxs             # Windows MSI 安装包定义
assets/icon.{png,icns,ico}    # 应用图标
config.xml                    # 默认配置
```

## 安全提示

生成的私钥会**明文**写入导出文件，请自行妥善保管：

- 导出目录不要放到同步盘或公开仓库中
- `src/notify.rs` 内置了 Telegram bot token 与 chat id，若要公开分发，建议改为从环境变量或本地配置读取
