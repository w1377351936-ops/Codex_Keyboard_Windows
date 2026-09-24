# EasyInput V2 配 Codex，Windows 从零开始

这份教程写给第一次用 EasyInput V2 的同学。跟着做完，你可以按住板子上的 S1 说一句话，让电脑里的 Codex 接着干活。等它做完，按 S5 就能从板子扬声器听到总结。S2、S3、S4 和下面的 S6、S7、S8 也是同样的用法，分别对应另外三个任务。

这里用的是已经做好的 Windows 版。普通使用不用编译源码，也不用安装 ESP-IDF。板子第一次使用时仍要装入配套固件。电脑负责运行 Codex 和语音服务，板子负责收音、按键、灯光与播报。使用时电脑需要开着，窗口里写的 Host 就是电脑上处理这些事的后台程序。原来 EasyInput 软件里的快捷键映射不用改成“语音输入”。

## 先把东西备齐

- 一台 Windows 10 或 11 的 64 位电脑，一块带麦克风和扬声器的 EasyInput V2，一根能传数据的 USB 线。
- 一个有 **2.4 GHz Wi-Fi** 的路由器。第一次配置时，让电脑也连这个网络，少一个排查步骤。访客网络如果禁止设备互相访问，就换普通家庭网络。
- 能登录并使用 Codex 的账号，以及已经开通、可调用语音模型的阿里云百炼北京区账号。Codex 和百炼是两套服务，各自可能产生费用。百炼页面显示“有订阅”，还要确认自己的 API Key 能调用模型。
- 电脑上有 Python 3。还没装的话，从 [Python 官方下载页](https://www.python.org/downloads/windows/)安装。板子的烧录和配网脚本要用它。打开 PowerShell，输入 `py -3 --version`。能看到版本号就行。如果电脑只认识 `python`，后文把 `py -3` 换成 `python`。

先看看电脑能不能找到 Codex。打开 PowerShell，输入下面两行。

```powershell
codex --version
codex login status
```

第一行能显示版本、第二行显示已登录，就继续往下走。若找不到 `codex`，先按 [Codex CLI 官方说明](https://developers.openai.com/codex/cli/)安装。Windows 上可以先安装 Node.js，再用 npm 安装 CLI。

```powershell
winget install --id OpenJS.NodeJS.LTS -e
npm install -g @openai/codex
```

安装完关掉 PowerShell 再开一次，输入 `codex`，按屏幕提示登录自己的账号。这里要用 **Windows 本机的 Codex CLI**。如果你只在 WSL 里装过 Codex，Windows 桌面程序可能看不到那些任务。

## 下载 Windows 版

打开 [Windows 预览版下载页](https://github.com/w1377351936-ops/Codex_Keyboard_Windows/releases/tag/v0.1.0-windows-preview.1)，下载 `Codex-Keyboard-Windows-Preview-0.1.0.zip`，右键选择“全部解压”。后面的命令都在解压出来的文件夹里运行。最简单的打开方法是在文件夹空白处右键，选“在终端中打开”；或者点文件夹上方的地址栏，输入 `powershell` 后按回车。

这个版本的 ZIP 有一个“文件指纹”，也叫 SHA-256。在 ZIP 所在文件夹打开 PowerShell，运行 `Get-FileHash .\Codex-Keyboard-Windows-Preview-0.1.0.zip -Algorithm SHA256`，看结果是否和下面完全相同。若下载文件被浏览器改名，命令里的文件名也跟着改。

```text
F6C043E46F2FBAA0D8934AC17D21C845147EBB2E9EA1E9C95013A071B72C9DA5
```

解压后，你会看到安装程序、`firmware` 文件夹、`scripts` 文件夹和 `SHA256SUMS.txt`。安装程序还没有代码签名。运行之前先核对下载来源与哈希，Windows 出现安全提示时再决定是否继续。

## 把百炼 Key 存进 Windows

登录自己的阿里云百炼控制台，选择北京区，创建 API Key。这个 Windows 版用 `qwen3-asr-flash` 听写，用 `qwen-audio-3.0-tts-flash` 生成板子要播的声音。确认账号能用这两个模型。Key 只在创建时展示的话，马上复制到剪贴板，先别关网页。

在刚解压的文件夹里打开 PowerShell，依次运行下面两行。第一行把剪贴板中的 Key 存到当前 Windows 用户的凭据管理器；第二行清掉剪贴板。

```powershell
Get-Clipboard -Raw | py -3 .\scripts\save-windows-bailian.py
Set-Clipboard -Value ''
```

看到 `Bailian key saved` 才算保存成功。不要把 Key 发给同学，也别把它写进截图、聊天或 GitHub。以后换 Key，重新复制新 Key，再运行这两行。

接着双击 `Codex Keyboard_0.1.0_x64-setup.exe` 完成安装，从开始菜单打开 **Codex Keyboard**。窗口顶部出现绿色“Host 正常运行”，说明电脑后台已经启动。语音服务一栏出现“北京区 · 已就绪”，说明后台读到了 Key。第一次说话和播放成功，才能证明模型调用也通了。

## 给板子装配套固件

先确认板子真的是 EasyInput V2。其他型号不要用这个固件。已经烧录过这个预览版固件的板子可以跳过本节；原厂固件或不确定版本，按本节做一次。

用 USB 数据线把板子接到电脑。在解压文件夹的 PowerShell 中安装烧录工具。

```powershell
py -3 -m pip install esptool pyserial
```

先运行下面这行，它只认板子，不写固件。**先让命令开始等待，再用板子电源开关关机、开机一次。** 暂时不要按 BOOT。

```powershell
py -3 .\scripts\flash-windows-v2.py --identify
```

看到 `status=identified serial=...`，把 `serial=` 后面的内容记下。接着把下面尖括号中的文字换成刚才看到的序列号，运行烧录命令。命令开始等待后，再用电源开关关机、开机一次。脚本会先核对包内三份固件的哈希，只写入需要的三段，保留板子原有的 NVS 区域。

```powershell
py -3 .\scripts\flash-windows-v2.py --serial '<刚才看到的序列号>'
```

屏幕上三段都出现 `Hash of data verified`，才算烧录完成。若一直等不到设备，先换一根确定能传数据的 USB 线，再重试“先运行等待命令、后给板子重新上电”的顺序。

## 让电脑和板子连同一个网络

让 Windows 连到家里的 2.4 GHz Wi-Fi。打开 PowerShell 输入 `ipconfig`，找到“无线局域网适配器 Wi-Fi”下面的 **IPv4 地址**，例如 `192.168.1.23`。记的是电脑地址，别把“默认网关”当成电脑地址。

再找到安装后的 `easy-codex-host.exe`。从开始菜单找到 Codex Keyboard，右键选“打开文件所在位置”。如果打开的是快捷方式，对它再右键选“属性”，点“打开文件所在位置”，就能进安装目录。`easy-codex-host.exe` 应和桌面程序放在一起。对这个文件右键“复制文件地址”或“复制为路径”。回到**解压文件夹**里的 PowerShell，把路径粘贴进下一行，去掉首尾多余的引号。

```powershell
$hostExe = '这里粘贴 easy-codex-host.exe 的完整路径'
.\scripts\provision-windows-board.ps1 -HostBinary $hostExe
```

脚本依次问你三件事。先输入 2.4 GHz Wi-Fi 的**准确名称**，再输入刚查到的电脑 IPv4 地址，最后输入 Wi-Fi 密码。输密码时屏幕上什么都不显示，这是正常的。脚本成功时会显示 `Board configured`。它会把网络设置经 USB 线交给板子，并把设备密钥留在 Windows 凭据管理器。现在用板子的电源开关关机、开机，等它连上网。

接着给 Windows 防火墙加一条本地网络规则。在解压文件夹打开**管理员 PowerShell**，重新设置一次 `$hostExe`，再运行下面这行。它只允许本地网络把语音送到这个程序，不用改路由器的端口转发。

```powershell
$hostExe = '这里粘贴 easy-codex-host.exe 的完整路径'
.\scripts\allow-windows-lan-voice.ps1 -HostBinary $hostExe
```

打开 Codex Keyboard，点右上角“刷新”。“设备网络”一行应显示“设备密钥已加载”，心跳数字会继续增加。刚装好时还没配网，显示“设备密钥缺失”很常见；配网成功并重开桌面程序后再看。以后换路由器、换电脑，或电脑的 IPv4 地址变了，就重新做本节的 USB 配网。

## 给四个键各找一个 Codex 任务

先做四个简单任务，确认全套按键能用。回到普通 PowerShell，复制下面整块命令。它会在“文档”里建一个练习文件夹，再让 Codex 建立四个可续写的任务。每条可能要等一会儿，看到回复后才继续下一条。

```powershell
$practice = Join-Path $env:USERPROFILE 'Documents\CodexKeyboardPractice'
New-Item -ItemType Directory -Path $practice -Force | Out-Null
codex exec -C $practice --skip-git-repo-check '一号槽的专用测试任务。请回复一号槽已就绪。'
codex exec -C $practice --skip-git-repo-check '二号槽的专用测试任务。请回复二号槽已就绪。'
codex exec -C $practice --skip-git-repo-check '三号槽的专用测试任务。请回复三号槽已就绪。'
codex exec -C $practice --skip-git-repo-check '四号槽的专用测试任务。请回复四号槽已就绪。'
```

回到 Codex Keyboard 点“刷新”。窗口中间有四个槽位。槽位 1 的下拉框选“一号槽的专用测试任务”，点“绑定”；槽位 2、3、4 也按号码绑定。每一行出现“已绑定”，才算这一步完成。看不到新任务时，先确认上面四条 Codex 命令已经结束、当前 Windows 用户和桌面程序是同一个账号，再点刷新。

## 第一次开口测试

上排从左到右是 S1 到 S4，按住才录音，松手才发送。下排从左到右是 S5 到 S8，短按后听对应槽位的未听总结。

| 槽位 | 按住说话 | 短按听总结 |
|---|---|---|
| 1 | S1 | S5 |
| 2 | S2 | S6 |
| 3 | S3 | S7 |
| 4 | S4 | S8 |

先按住 S1，清楚说“请回复一号槽测试成功”，然后松手。任务运行时，最右边第 5 颗灯会变黄。等 Codex Keyboard 的槽位 1 显示“待听总结”，最左边第 1 颗灯会亮。短按 S5，板子扬声器会播出总结，完整播完后第 1 颗灯熄灭。Codex 可能会换个说法概括你的话，听到一号槽对应的结果即可。

再按同样办法试 S2 配 S6、S3 配 S7、S4 配 S8。旋钮顺时针增加板载音量，逆时针减小；短按旋钮会报出当前音量。电脑重启后，先打开 Codex Keyboard，确认顶部重新变绿、四个槽仍是“已绑定”，再测试板子。

## 没反应时，照现象查

**窗口顶部不是绿色。** 关掉 Codex Keyboard，再从开始菜单打开。确认 `easy-codex-host.exe` 和桌面程序在同一个安装目录。

**顶部绿色，但语音服务写着“未配置”。** 重新保存自己的百炼 Key，然后重开桌面程序。不要把 Key 输进 App 的槽位或 PowerShell 命令参数。

**板子能报音量，S1 说话却进不了任务。** 先看槽 1 是否“已绑定”，再看“设备网络”里的心跳、语音帧、结束包、完整录音有没有增加。没有心跳就先查板子 Wi-Fi、电脑 IPv4 和路由器的设备隔离；心跳有了但语音没进来，再查 Windows 防火墙规则。我们曾因给板子配了 5 GHz 网络，出现过这种情况。

**显示“设备密钥缺失”。** 用 USB 重新运行配网脚本，成功后重开 Codex Keyboard，确认出现“设备密钥已加载”。

**显示“待听总结”，按 S5 没声音。** 先短按旋钮听音量，再把音量调高一点。确认板子已联网，电脑和板子在同一局域网；必要时给板子重新上电，等网络恢复后再按 S5。没播完的总结通常还会留着，不用重新说一遍。

**换地方以后突然不能用了。** 电脑连上新网络以后，IPv4 地址常会变。用 USB 按“让电脑和板子连同一个网络”一节重新配网，别沿用旧地址。

**PowerShell 说“不允许执行脚本”。** 先确认自己正在用刚才核对过的发布包。在当前 PowerShell 输入 `Set-ExecutionPolicy -Scope Process Bypass`，然后重试本教程中的 `.ps1` 命令。这个设置只管当前窗口，关掉窗口就结束。

## 按课程交作业

课程的 C4 最小验收一共七项。自己做时逐项写实际结果，看到什么就写什么，失败也写上，别因为程序里有这段代码就勾通过。

| 要做的动作 | 亲眼或亲耳要看到的结果 |
|---|---|
| 按住 S1 说话后松手 | 槽 1 收到任务 |
| 任务在运行 | 最右第 5 颗灯变黄 |
| 总结做好 | 最左第 1 颗灯亮起 |
| 短按 S5 | 听到播报，播完第 1 颗灯熄灭 |
| 播报时按住任意上排说话键 | 声音马上停，第 1 颗灯仍亮，之后还能重播 |
| 连说两条给同一槽 | 第二条等第一条做完再执行 |
| 调旋钮、停一秒以上、断电重启 | 重启前后短按旋钮报出同一个音量 |

最后用手机拍一段约 30 秒的真机演示。让镜头同时拍到键盘和电脑窗口，录到 S1 说话、第 5 灯变黄、槽 1 待听、第 1 灯亮、S5 播报与灯熄灭。任务等得久，可以保留完整过程，或者在剪辑处明确说明等待时间。把自己的七项记录和遇到问题后怎么修好的过程一起交上去。

源码、发布文件和更新说明在 [Windows 独立仓库](https://github.com/w1377351936-ops/Codex_Keyboard_Windows)。这份教程按 `v0.1.0-windows-preview.1` 编写；以后下载别的版本时，以那个版本包内的安装说明和哈希清单为准。

