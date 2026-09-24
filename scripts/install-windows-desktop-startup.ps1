param(
    [ValidateSet('Install', 'Status', 'Uninstall')]
    [string]$Action = 'Install',
    [string]$DesktopBinary
)

$ErrorActionPreference = 'Stop'
$startupDirectory = [Environment]::GetFolderPath('Startup')
if ([string]::IsNullOrWhiteSpace($startupDirectory)) {
    throw 'Current user Startup folder is unavailable.'
}
$shortcutPath = Join-Path $startupDirectory 'Codex Keyboard.lnk'
$shell = New-Object -ComObject WScript.Shell

if ($Action -eq 'Status') {
    if (-not (Test-Path -LiteralPath $shortcutPath -PathType Leaf)) {
        Write-Output 'startup=absent'
        return
    }
    $shortcut = $shell.CreateShortcut($shortcutPath)
    Write-Output 'startup=present'
    Write-Output "target_present=$(Test-Path -LiteralPath $shortcut.TargetPath -PathType Leaf)"
    return
}

if ($Action -eq 'Uninstall') {
    if (Test-Path -LiteralPath $shortcutPath -PathType Leaf) {
        $shortcut = $shell.CreateShortcut($shortcutPath)
        if ($shortcut.Description -ne 'Start Codex Keyboard and its local Host at sign-in') {
            throw 'Startup shortcut is not owned by this installer.'
        }
        Remove-Item -LiteralPath $shortcutPath -Force
    }
    Write-Output 'startup=uninstalled'
    return
}

if ([string]::IsNullOrWhiteSpace($DesktopBinary)) {
    throw 'Provide -DesktopBinary with the installed Desktop executable path.'
}
if (-not [IO.Path]::IsPathFullyQualified($DesktopBinary)) {
    throw 'Desktop binary path must be absolute.'
}
# Preserve the caller's path. A sandboxed launcher may remap Resolve-Path to another profile.
$target = [IO.Path]::GetFullPath($DesktopBinary)
if (-not (Test-Path -LiteralPath $target -PathType Leaf)) {
    throw 'Desktop binary is missing.'
}
if (Test-Path -LiteralPath $shortcutPath -PathType Leaf) {
    $existing = $shell.CreateShortcut($shortcutPath)
    if ($existing.Description -ne 'Start Codex Keyboard and its local Host at sign-in') {
        throw 'Startup shortcut is not owned by this installer.'
    }
}
$shortcut = $shell.CreateShortcut($shortcutPath)
$shortcut.TargetPath = $target
$shortcut.WorkingDirectory = Split-Path -Path $target -Parent
$shortcut.Description = 'Start Codex Keyboard and its local Host at sign-in'
$shortcut.WindowStyle = 1
$shortcut.Save()
$saved = $shell.CreateShortcut($shortcutPath)
if ($saved.TargetPath -ne $target) {
    throw 'Startup shortcut target did not round-trip.'
}
Write-Output 'startup=installed'
Write-Output 'target_present=True'
