#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Install AdNox as a boot-time DNS blocker on Windows and point system DNS at it.
.DESCRIPTION
    Copies adnox.exe into Program Files, downloads filter lists, registers a scheduled task that
    runs `adnox run` at startup as SYSTEM, and sets every active adapter's DNS to 127.0.0.1.
#>

$ErrorActionPreference = 'Stop'

$InstallDir = Join-Path $env:ProgramFiles 'AdNox'
$ListsDir   = Join-Path $InstallDir 'lists'
$TaskName   = 'AdNox DNS Blocker'

# Locate the built binary (release preferred, then debug).
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$candidates = @(
    (Join-Path $here '..\engine\target\release\adnox.exe'),
    (Join-Path $here '..\engine\target\debug\adnox.exe'),
    (Join-Path $here 'adnox.exe')
)
$src = $candidates | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $src) {
    throw "adnox.exe not found. Build it first:  cd engine; cargo build --release -p adnox-cli"
}

Write-Host "Installing AdNox to $InstallDir ..."
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
New-Item -ItemType Directory -Force -Path $ListsDir   | Out-Null
Copy-Item $src (Join-Path $InstallDir 'adnox.exe') -Force

$exe = Join-Path $InstallDir 'adnox.exe'

Write-Host "Downloading filter lists ..."
& $exe update --dir $ListsDir

Write-Host "Registering boot task '$TaskName' ..."
$action    = New-ScheduledTaskAction -Execute $exe `
    -Argument "run --dir `"$ListsDir`" --bind 127.0.0.1:53 --upstream 1.1.1.1:53"
$trigger   = New-ScheduledTaskTrigger -AtStartup
$principal = New-ScheduledTaskPrincipal -UserId 'SYSTEM' -LogonType ServiceAccount -RunLevel Highest
$settings  = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries `
    -RestartCount 3 -RestartInterval (New-TimeSpan -Minutes 1)

Register-ScheduledTask -TaskName $TaskName -Action $action -Trigger $trigger `
    -Principal $principal -Settings $settings -Force | Out-Null
Start-ScheduledTask -TaskName $TaskName

Write-Host "Pointing active adapters' DNS at 127.0.0.1 ..."
Get-DnsClientServerAddress -AddressFamily IPv4 |
    Where-Object { $_.ServerAddresses } |
    Select-Object -ExpandProperty InterfaceIndex -Unique |
    ForEach-Object {
        try { Set-DnsClientServerAddress -InterfaceIndex $_ -ServerAddresses '127.0.0.1' } catch {}
    }
Clear-DnsClientCache

Write-Host ""
Write-Host "AdNox installed. Test it with:" -ForegroundColor Green
Write-Host "    Resolve-DnsName doubleclick.net   # -> 0.0.0.0 (blocked)"
Write-Host "    Resolve-DnsName example.com       # -> resolves normally"
