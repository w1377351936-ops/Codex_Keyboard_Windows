param(
    [Parameter(Mandatory = $true)]
    [string]$HostBinary
)

$ErrorActionPreference = 'Stop'
if (-not [IO.Path]::IsPathFullyQualified($HostBinary)) {
    throw 'HostBinary must be an absolute path.'
}
$binary = [IO.Path]::GetFullPath($HostBinary)
if (-not (Test-Path -LiteralPath $binary -PathType Leaf)) {
    throw 'Host binary does not exist.'
}
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this firewall setup from an administrator PowerShell.'
}

$ruleName = 'Codex Keyboard LAN Voice'
$existing = @(Get-NetFirewallRule -DisplayName $ruleName -ErrorAction SilentlyContinue)
if ($existing.Count -gt 1) {
    throw 'Multiple matching firewall rules exist; review them manually.'
}
if ($existing.Count -eq 1) {
    $application = $existing[0] | Get-NetFirewallApplicationFilter
    $port = $existing[0] | Get-NetFirewallPortFilter
    $address = $existing[0] | Get-NetFirewallAddressFilter
    if ($application.Program -ne $binary -or $port.Protocol -ne 'UDP' -or
        $port.LocalPort -ne '17333' -or $address.RemoteAddress -ne 'LocalSubnet') {
        throw 'An existing rule with this name has different settings; review it manually.'
    }
    Write-Output 'firewall_rule=already_present'
    return
}

New-NetFirewallRule -DisplayName $ruleName -Direction Inbound -Action Allow `
    -Program $binary -Protocol UDP -LocalPort 17333 -RemoteAddress LocalSubnet `
    -Profile Any | Out-Null
Write-Output 'firewall_rule=created_for_local_subnet'
