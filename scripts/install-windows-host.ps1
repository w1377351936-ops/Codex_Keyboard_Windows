param(
    [ValidateSet('Install', 'Status', 'Uninstall')]
    [string]$Action = 'Install',
    [string]$SourceBinary = (Join-Path $PSScriptRoot '..\target\release\easy-codex-host.exe')
)

$ErrorActionPreference = 'Stop'
$taskName = 'Codex Keyboard Host'
$appRoot = Join-Path $env:LOCALAPPDATA 'EasyCodexInput'
$binDirectory = Join-Path $appRoot 'bin'
$installedBinary = Join-Path $binDirectory 'easy-codex-host.exe'

function Get-HostTask {
    Get-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
}

if ($Action -eq 'Status') {
    $task = Get-HostTask
    if ($null -eq $task) {
        Write-Output 'task=absent'
    } else {
        Write-Output "task=$($task.State)"
    }
    $statusBinary = if ($null -ne $task) { $task.Actions[0].Execute } else { $installedBinary }
    Write-Output "binary_present=$(Test-Path -LiteralPath $statusBinary -PathType Leaf)"
    if (Test-Path -LiteralPath $statusBinary -PathType Leaf) {
        $previousPreference = $ErrorActionPreference
        $ErrorActionPreference = 'SilentlyContinue'
        try {
            $health = & $statusBinary health 2>$null
            $healthy = $LASTEXITCODE -eq 0
        } catch {
            $healthy = $false
        } finally {
            $ErrorActionPreference = $previousPreference
        }
        if ($healthy) { $health } else { Write-Output 'health=unavailable' }
    }
    return
}

if ($Action -eq 'Uninstall') {
    $task = Get-HostTask
    if ($null -ne $task) {
        Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
        Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    }
    if (Test-Path -LiteralPath $installedBinary -PathType Leaf) {
        Remove-Item -LiteralPath $installedBinary -Force
    }
    Write-Output 'status=uninstalled'
    return
}

$source = (Resolve-Path -LiteralPath $SourceBinary).Path
if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
    throw 'Host binary is missing; build easy-codex-host in release mode first.'
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent().Name
if ([string]::IsNullOrWhiteSpace($identity)) {
    throw 'Current Windows identity is unavailable.'
}

New-Item -ItemType Directory -Path $binDirectory -Force | Out-Null
$sourceHash = (Get-FileHash -LiteralPath $source -Algorithm SHA256).Hash
if (-not (Test-Path -LiteralPath $installedBinary -PathType Leaf) -or
    (Get-FileHash -LiteralPath $installedBinary -Algorithm SHA256).Hash -ne $sourceHash) {
    $running = Get-CimInstance Win32_Process -Filter "Name='easy-codex-host.exe'" |
        Where-Object { $_.ExecutablePath -eq $installedBinary }
    if ($running) {
        throw 'Installed Host is running. Stop its scheduled task before upgrading the binary.'
    }
    Copy-Item -LiteralPath $source -Destination $installedBinary -Force
}

$scheduledAction = New-ScheduledTaskAction -Execute $installedBinary -Argument 'daemon' -WorkingDirectory $appRoot
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $identity
$principal = New-ScheduledTaskPrincipal -UserId $identity -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet -MultipleInstances IgnoreNew -RestartCount 3 `
    -RestartInterval (New-TimeSpan -Minutes 1) -ExecutionTimeLimit (New-TimeSpan -Seconds 0) `
    -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries
Register-ScheduledTask -TaskName $taskName -Action $scheduledAction -Trigger $trigger `
    -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $taskName
for ($attempt = 0; $attempt -lt 20; $attempt++) {
    $previousPreference = $ErrorActionPreference
    $ErrorActionPreference = 'SilentlyContinue'
    try {
        & $installedBinary health *> $null
        $healthy = $LASTEXITCODE -eq 0
    } catch {
        $healthy = $false
    } finally {
        $ErrorActionPreference = $previousPreference
    }
    if ($healthy) { break }
    Start-Sleep -Milliseconds 250
}
if (-not $healthy) {
    Stop-ScheduledTask -TaskName $taskName -ErrorAction SilentlyContinue
    Unregister-ScheduledTask -TaskName $taskName -Confirm:$false
    throw 'Host did not become healthy after scheduled task activation.'
}
Write-Output 'status=installed'
Write-Output "sha256=$sourceHash"
Write-Output "account=$identity"
