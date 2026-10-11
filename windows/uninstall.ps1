#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Remove AdNox: restore DHCP DNS, stop/remove the scheduled task, delete the install dir.
#>

$ErrorActionPreference = 'SilentlyContinue'

$InstallDir = Join-Path $env:ProgramFiles 'AdNox'
$TaskName   = 'AdNox DNS Blocker'

Write-Host "Restoring automatic (DHCP) DNS on all adapters ..."
Get-DnsClientServerAddress -AddressFamily IPv4 |
    Select-Object -ExpandProperty InterfaceIndex -Unique |
    ForEach-Object { Set-DnsClientServerAddress -InterfaceIndex $_ -ResetServerAddresses }
Clear-DnsClientCache

Write-Host "Stopping and removing scheduled task '$TaskName' ..."
Stop-ScheduledTask    -TaskName $TaskName
Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false

# Ensure no stray process keeps port 53.
Get-Process adnox -ErrorAction SilentlyContinue | Stop-Process -Force

Write-Host "Deleting $InstallDir ..."
Remove-Item -Recurse -Force $InstallDir

Write-Host "AdNox removed." -ForegroundColor Green
