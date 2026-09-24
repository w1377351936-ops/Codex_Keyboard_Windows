# Windows + 火山引擎课程复刻

## 目标

以 `v0.1.0-local-training` 为基线，在 Windows 电脑和 EasyInput V2 上复现四槽语音输入、Codex 严格 FIFO、权威完成总结、语音信箱播放与 exact heard 的完整链路。保持设备 LAN 协议和固件数据语义。用户已发现北京地域百炼入门型 AI 通用节省计划，优先复用课程原有 `qwen3-asr-flash` 与 `qwen-audio-3.0-tts-flash`；火山引擎 TTS 试用保留作备选。

## 本机已知条件

- Windows 有 Codex CLI、Node、VS Build Tools 2022、ESP-IDF v5.5.5 与 ESP-IDF Python 环境。
- 桌面 IDF 快捷方式指向 v5.4.1，构建时必须显式选择 v5.5.5。
- Rust 1.94 MSVC 工具链安装在工作区 `.work/codex-toolchain`。
- USB 只读探针已确认 `USB\\VID_303A&PID_1006\\EASY-INPUT-V2`；不能把构建当作真机验收。
- 用户已选择纯 Windows 原生 Host。现有 Ubuntu WSL2 为 NAT 网络，仅作技术备选，不用作本次运行时。

## 实现顺序

1. **编译基线**：在 Windows 运行 `cargo check`，记录所有 Unix 专属接口和目标平台依赖；先保留原 macOS 代码路径，再引入平台模块。每个阶段运行受影响的测试。
2. **私有持久化**：Windows 数据根使用当前用户 `LOCALAPPDATA/EasyCodexInput`。以 Windows ACL/当前用户身份与文件重解析点检查替代 Unix mode/openat；百炼密钥使用当前用户 Windows Credential Manager 的 `CRED_TYPE_GENERIC`，目标名 `EasyCodexInput/DASHSCOPE_API_KEY`。Windows 控制面板“添加 Windows 凭据”产生的 `CRED_TYPE_DOMAIN_PASSWORD` 在本机 `CredReadW` 中返回零长度 blob，不能用于 Host。原子文件替换继续先写临时文件、落盘、再提交。绝不在日志输出 secret。
3. **Host 运行**：以当前用户权限启动单实例 Host；用受限本地 IPC 实现 health/dashboard/bind，不经公网监听。Codex `exec resume --json` 仍以 stdin 传 prompt，进程退出与 rollout `task_complete` 仍由原状态机处理。
4. **桌面端**：接通 Tauri Windows 命令和现有前端，构建安装包；状态和绑定继续只通过常驻 Host IPC。
5. **语音服务**：沿用仓库北京地域百炼 ASR/TTS 状态机；将硬编码的旧 DashScope 主机改为可验证的业务空间专属域名，同时保持 ASR `/compatible-mode/v1/chat/completions` 与 TTS `/api/v1/services/audio/tts/SpeechSynthesizer` 的各自路径。输出仍须是完整可校验的 48 kHz mono PCM16 WAV。网络提交状态不明确时不自动重发；旧未听 generation 保留。火山 TTS 仅作为后续可选适配。
6. **固件与真机**：使用 IDF 5.5.5 构建 esp32s3、16 MB；核对 USB 设备身份后生成候选镜像 SHA。烧录前单独取得三份精确 SHA 授权，保留 NVS。最后用四槽按键、灯和扬声器验收，重点检查播放中 PTT 抢占与旧 ACK 拒绝。

## 当前下一步

首次 Windows 基线的 422 个编译错误已收敛：Host 和 Desktop 均可构建。Host health/dashboard 已实际启动验证，Windows 凭据、缓存、Luna 总结和百炼 TTS 已单独通过。NSIS 安装包包含 Desktop 与 Host，静默安装/卸载测试通过。百炼共享北京 API 域名经真实 TTS/ASR 验证可用，当前不必强制切换专属域名。下一步是连接开发板、完成 Host 登录常驻与桌面绑定验收，随后在得到精确三镜像 SHA 授权后烧录并完成真机闭环。服务凭据只存当前用户凭据管理器，不提交仓库。

此前本机 USB 只读探针识别过 `USB\\VID_303A&PID_1006\\EASY-INPUT-V2` HID；本轮 `board-status` 返回零候选。未复位或烧录设备。
