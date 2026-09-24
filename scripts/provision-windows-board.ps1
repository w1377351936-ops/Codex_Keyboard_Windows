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

$boardStatus = & $binary board-status
if ($LASTEXITCODE -ne 0 -or $boardStatus -notcontains 'usb_candidates=1') {
    throw 'Connect exactly one EasyInput V2 by USB before provisioning.'
}

$ssid = Read-Host '2.4 GHz Wi-Fi SSID'
$ssidBytes = [Text.Encoding]::UTF8.GetByteCount($ssid)
if ($ssidBytes -lt 1 -or $ssidBytes -gt 32) {
    throw 'SSID must be 1..32 UTF-8 bytes.'
}

$hostAddress = Read-Host 'This computer LAN IPv4 address'
$parsedAddress = $null
if (-not [Net.IPAddress]::TryParse($hostAddress, [ref]$parsedAddress) -or
    $parsedAddress.AddressFamily -ne [Net.Sockets.AddressFamily]::InterNetwork -or
    [Net.IPAddress]::IsLoopback($parsedAddress) -or
    $parsedAddress.Equals([Net.IPAddress]::Any)) {
    throw 'Enter a reachable LAN IPv4 address of this computer.'
}

$securePassword = Read-Host 'Wi-Fi password (hidden)' -AsSecureString
$bstr = [IntPtr]::Zero
$password = $null
$previousEncoding = $OutputEncoding
try {
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($securePassword)
    $password = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    $OutputEncoding = [Text.UTF8Encoding]::new($false)
    $password | & $binary provision-lan $ssid $parsedAddress.ToString() 17333
    if ($LASTEXITCODE -ne 0) {
        throw 'Board provisioning failed.'
    }
} finally {
    $OutputEncoding = $previousEncoding
    $password = $null
    if ($bstr -ne [IntPtr]::Zero) {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    }
    if ($null -ne $securePassword) {
        $securePassword.Dispose()
    }
}

& $binary share-device-secret
if ($LASTEXITCODE -ne 0) {
    throw 'The board was configured, but the device secret could not be preserved in Windows Credential Manager.'
}
Write-Output 'Board configured. Power it off and on once, then check the Desktop device network status.'
