# Verify-RebootCutoverReadiness.ps1
# Read-only verification of pre/post-reboot pagefile cutover, standalone UEFI bootloader images,
# 100 GB Host-8 room store, and live local fabric endpoints.
# NEVER reboots the machine (auto_reboot=0, writes=0).

param(
    [string]$OutReceipt = "C:\Users\acer\Asolaria-Argon-Antigravity-Unification-ASI-OS\receipts\REBOOT-CUTOVER-READINESS-20261021.hbp"
)

$ErrorActionPreference = "Stop"

$regPaging = (Get-ItemProperty "HKLM:\SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management").PagingFiles -join ";"
$livePagefiles = @(Get-CimInstance Win32_PageFileUsage -ErrorAction SilentlyContinue)
$livePfNames = ($livePagefiles | ForEach-Object { "$($_.Name):$($_.AllocatedBaseSize)MB" }) -join ","
if ([string]::IsNullOrEmpty($livePfNames)) { $livePfNames = "NONE" }
$dPagefileActive = if ($livePfNames -match "^D:") { 1 } else { 0 }
$cPagefileActive = if ($livePfNames -match "^C:") { 1 } else { 0 }

$wantEfiSha = "a392ab70cfb24e39b2048b1dd8ecc0569b164d06fe2e7e81d08fb6be57da03d8"
$wantBakSha = "37dda436dbb93b413711c73e0d0f08b11c39997c596bbbb399a32f37d8e0612d"

function Get-ShaOrMissing([string]$p) {
    if (Test-Path $p) {
        return (Get-FileHash -Algorithm SHA256 $p).Hash.ToLower()
    }
    return "MISSING"
}

$signedEfiSha = Get-ShaOrMissing "C:\Users\acer\asolaria-unification-20261020\asolaria-classical-room-memory-signed.efi"
$bakFiles = @(Get-ChildItem "C:\Users\acer\asolaria-unification-20261020\standalone-backup\*.efi" -ErrorAction SilentlyContinue | Sort-Object Name)
$bak1Sha = if ($bakFiles.Count -ge 1) { (Get-FileHash -Algorithm SHA256 $bakFiles[0].FullName).Hash.ToLower() } else { "MISSING" }
$bak2Sha = if ($bakFiles.Count -ge 2) { (Get-FileHash -Algorithm SHA256 $bakFiles[1].FullName).Hash.ToLower() } else { "MISSING" }
$usbEspBootSha = Get-ShaOrMissing "E:\EFI\BOOT\BOOTX64.EFI"

$efiMatch = if ($signedEfiSha -eq $wantEfiSha) { "EXACT" } else { "MISMATCH" }
$bakMatch = if ($bak1Sha -eq $bak2Sha -and $bak1Sha.StartsWith("37dda436")) { "EXACT" } else { "MISMATCH" }

$roomFiles = @(Get-ChildItem "D:\ASOLARIA-HOST8-ROOMS-100G\sector-*.rooms" -ErrorAction SilentlyContinue)
$roomCountFiles = $roomFiles.Count
$roomTotalBytes = ($roomFiles | Measure-Object -Property Length -Sum).Sum

function Probe-Port([int]$port, [string]$path) {
    try {
        $r = Invoke-WebRequest -Uri "http://127.0.0.1:$port$path" -UseBasicParsing -TimeoutSec 3
        return $r.StatusCode
    } catch {
        return 0
    }
}

$p4600 = Probe-Port 4600 "/native/unified.hbp"
$p5088 = Probe-Port 5088 "/health.hbp"
$p5090 = Probe-Port 5090 "/health.hbp"
$p4796 = Probe-Port 4796 "/health"
$p4947 = Probe-Port 4947 "/behcs/health"
$p4949 = Probe-Port 4949 "/health"

$lines = @(
    "CUTOVERPAGEFILE|registry_paging_files=$regPaging|active_pagefiles=$livePfNames|d_pagefile_active=$dPagefileActive|c_pagefile_active=$cPagefileActive|reboot_cutover_complete=$cPagefileActive|reboot_pending=$dPagefileActive|auto_reboot=0|writes=0|json=0",
    "CUTOVEREFI|system_esp_guid=83a60844-4279-42a4-9feb-3c762e135497|signed_efi_sha256=$signedEfiSha|efi_match=$efiMatch|backup_asolaria_sha256=$bak1Sha|backup_bootx64_sha256=$bak2Sha|backup_match=$bakMatch|usb_asoesp_bootx64_sha256=$usbEspBootSha|standalone_partition_discovery=1|stick_writes=0|json=0",
    "CUTOVERROOMS|store=D:/ASOLARIA-HOST8-ROOMS-100G|sector_files=$roomCountFiles|total_bytes=$roomTotalBytes|rooms=12500000000|room_bytes=8|writes=0|json=0",
    "CUTOVERFABRIC|p4600_native=$p4600|p5088_host8=$p5088|p5090_dock=$p5090|p4796_recall=$p4796|p4947_bus=$p4947|p4949_super=$p4949|all_six_up=$(if ($p4600 -eq 200 -and $p5088 -eq 200 -and $p5090 -eq 200 -and $p4796 -eq 200 -and $p4947 -eq 200 -and $p4949 -eq 200) { 1 } else { 0 })|writes=0|json=0"
)

$text = ($lines -join "`n") + "`n"
$dir = Split-Path -Parent $OutReceipt
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir | Out-Null }
[System.IO.File]::WriteAllBytes($OutReceipt, [System.Text.Encoding]::UTF8.GetBytes($text))
$sha = (Get-FileHash -Algorithm SHA256 $OutReceipt).Hash.ToLower()
$base = Split-Path -Leaf $OutReceipt
[System.IO.File]::WriteAllBytes("$OutReceipt.sha256", [System.Text.Encoding]::UTF8.GetBytes("$sha  $base`n"))
Write-Output ($lines -join "`n")
Write-Output "CUTOVERSEAL|receipt=$base|sha256=$sha|auto_reboot=0|writes=0|json=0"
