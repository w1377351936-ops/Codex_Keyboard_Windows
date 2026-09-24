# Windows 安装与配网（EasyInput V2）

本指南用于 Windows 10/11 x64、EasyInput V2（ESP32-S3，带麦克风和扬声器）。键盘和电脑须在同一个可互访的局域网；开发板 Wi-Fi 使用 2.4 GHz。所有账号、密钥、Wi-Fi 密码和任务绑定由使用者自行配置，发布包里没有预置值。

## 准备

1. 安装并登录 Codex CLI。先在 PowerShell 运行 `codex --version`，确认命令可用；在 CLI 中创建四个可以由 CLI 继续对话的任务。Codex 桌面程序独占写入的任务可能导致 CLI `resume` 冲突。
2. 准备自己的阿里云百炼北京区 API Key，确认账号可调用本项目使用的 ASR/TTS 模型。语音服务和 Codex 的调用由各自账号计费。
3. 下载本项目 Windows 发布包。用包内 `SHA256SUMS.txt` 核对安装程序和三个固件镜像；核对后再安装或烧录。

## 安装桌面程序与配置百炼

运行发布包中的 `Codex Keyboard_0.1.0_x64-setup.exe`。先完成下面的百炼 Key 配置，再从开始菜单首次打开 **Codex Keyboard**；桌面程序会启动同目录的本机 Host。页面顶部绿色“Host 正常运行”表示后台进程可访问。

发布包附带 `scripts/save-windows-bailian.py`，需要 Windows 的 Python 3。将自己的百炼 API Key 复制到剪贴板，在发布包解压目录运行：

```powershell
Get-Clipboard -Raw | py -3 .\scripts\save-windows-bailian.py
Set-Clipboard -Value ''
```

脚本从标准输入读取 Key，保存到当前 Windows 用户的凭据管理器，不在命令行参数、仓库或发布包里写入 Key。若 `py` 不可用，用本机 Python 3 可执行文件替换 `py -3`。配置后首次打开 Codex Keyboard，页面应显示“北京区 · 已就绪”。如果之前已经打开过 Host，请先退出运行中的 `easy-codex-host.exe`，再打开桌面程序，让 Host 重新读取凭据。

## 安装开发板固件

板型必须确认是 EasyInput V2，不要将镜像烧录到其他型号。发布包内的 `firmware/build/` 包含 `bootloader.bin`、`partition-table.bin` 和 `easy_codex_input.bin`，包内 `scripts/flash-windows-v2.py` 会先逐个核对固定 SHA-256，并只写入 `0x0`、`0x8000`、`0x10000`，保留 NVS。烧录需要 ESP32-S3 USB 数据线、Python `esptool` 和 `pyserial`；可用 `py -3 -m pip install esptool pyserial` 安装依赖。按脚本提示确认设备序列号后运行。构建源码需 ESP-IDF v5.5.5，参见 `scripts/build-windows-firmware.ps1`。

先在发布包解压目录运行只读识别模式，再用板子电源开关关机、开机一次；它只报告瞬时 ESP32-S3 USB 序列号，不烧录：

```powershell
py -3 .\scripts\flash-windows-v2.py --identify
```

确认板型和包内三份镜像 SHA-256 后，用上一步显示的序列号启动烧录等待，再关机、开机一次；默认不按 BOOT：

```powershell
py -3 .\scripts\flash-windows-v2.py --serial '<上一步显示的序列号>'
```

脚本输出每段 `Hash of data verified` 后，等待应用 USB 设备重新出现。

## 配置本地网络

将 Windows 电脑连到要给开发板使用的 2.4 GHz Wi-Fi。找到安装目录里的 `easy-codex-host.exe`；从开始菜单的 Codex Keyboard 快捷方式打开文件位置即可定位。使用发布包中的脚本，在 PowerShell 中运行：

```powershell
.\scripts\provision-windows-board.ps1 -HostBinary '安装目录\easy-codex-host.exe'
```

脚本要求输入 2.4 GHz SSID、电脑在此局域网内的 IPv4 地址和 Wi-Fi 密码。密码输入不回显，只经标准输入交给本机 Host；Host 通过 USB HID 下发给已连接的开发板。板子保存成功后，用电源开关关机再开机，等待联网。换 Wi-Fi、电脑或电脑地址后需重新配网。

Host 接收板子语音使用 UDP 17333。若 Windows 防火墙阻止入站，在管理员 PowerShell 中运行：

```powershell
.\scripts\allow-windows-lan-voice.ps1 -HostBinary '安装目录\easy-codex-host.exe'
```

此规则只允许该程序从本地子网接收 UDP 17333；不需要路由器端口转发。

## 绑定和使用

打开 Codex Keyboard，确认顶部 Host 正常运行、百炼已就绪，设备网络里显示“设备密钥已加载”。在四个槽位分别选择自己由 Codex CLI 创建的任务，点击“绑定”。按住 S1–S4 说话，松开后等待槽位出现“待听总结”；短按对应 S5–S8 收听。旋钮调节板载音量，短按旋钮播报音量。

若刚安装时桌面程序显示离线，先关闭后重新打开；若板子收不到语音，核对双方网络、电脑当前 IPv4、防火墙规则和“设备密钥已加载”。板子完整播报结束后，槽位应回到“已绑定”。

## 已知限制

- 此预览版已经在一台 Windows 电脑和 EasyInput V2 上完成四槽真机闭环；其他电脑和网络仍需自行复测。
- 登录后自动启动与课程目标中的每键 20 轮长稳尚未完成验收。关闭桌面窗口不会主动停止已经运行的 Host；重启电脑后请检查 Host 是否绿色。
- 部分网络下第 5 颗灯静置仍显示蓝色；状态同步原因未定位，语音和播报链路已独立验证。
- 安装包尚未代码签名。请核对发布包里的 SHA-256，再决定是否运行。
