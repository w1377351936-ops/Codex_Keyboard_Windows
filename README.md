# Codex Keyboard

EasyInput V2 的本地语音任务键盘。上排 S1–S4 按住说话，把要求交给各自绑定的 Codex 任务；下排 S5–S8 在开发板扬声器播放对应任务的未听总结。键盘与电脑通过同一局域网通信，电脑运行 Host 和桌面管理程序。

## Windows 版

Windows 10/11 x64 的原生 Host 和桌面程序已完成四槽真机闭环。从 [Windows 预览版 Release](https://github.com/w1377351936-ops/Codex_Keyboard_Windows/releases/tag/v0.1.0-windows-preview.1) 下载 ZIP；第一次用可照着 [Windows 新手培训教程](docs/Windows新手培训教程.md)操作，命令和安装细节见 [Windows 安装与配网](docs/windows-setup.md)。发布包包含 Windows 安装程序和 EasyInput V2 对应的固件镜像；每位使用者自行配置 Codex 登录、百炼 API Key、任务绑定和自己的 2.4 GHz Wi-Fi。发布包不包含这些个人数据。

当前为预览版：四槽输入、总结和板载播报、播放抢占、Host 重启后的未听恢复已有真机验证；登录自启、各键 20 轮长稳和第 5 颗灯在部分网络下的空闲颜色仍待验收。使用前请看指南中的已知限制。

## macOS 版与源码

本项目最初是 macOS 本地 Wi-Fi 版本。Windows 和 macOS 共用 Rust Host 核心、Tauri 2 桌面界面及 ESP32-S3 固件。源码目录：`app/host/`、`app/desktop/`、`firmware/`。架构和协议分别见 [总体方案](docs/总体方案.md)、[端到端架构](docs/端到端架构.md) 和 [协议草案](docs/协议草案.md)。

macOS 版的键位和语音流程相同；其 LaunchAgent、Keychain 和安装方式与 Windows 不同。Windows 请以 [Windows 安装与配网](docs/windows-setup.md) 为准。

## 来源与许可

本仓从 [`easy-codex-input@52949d3`](https://github.com/Larkspur-Wang/easy-codex-input) 抽取；固件来源和许可边界见 [firmware/UPSTREAM.md](firmware/UPSTREAM.md)。本仓代码采用 [Apache-2.0](LICENSE)。
