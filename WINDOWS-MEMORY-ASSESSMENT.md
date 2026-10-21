# Windows-wide memory — SSD paging configured; USB preserved

## UPDATE — SSD paging configured, pending next Windows start

After the operator's CONTINUE, administrator-approved helper9856 configured `C:/pagefile.sys 8192 16384` at UTC2026-10-20T22:34:01. Windows registry readback and `Win32_PageFileSetting` independently agree. Automatic management remainsFalse. The live `Win32_PageFileUsage` and `ExistingPageFiles` still show D:, allocated8192 MiB, usage787 MiB at the post-change check. **No reboot was performed; C: paging activation and performance after the next Windows start remain NOT_MEASURED.** No pagefile data was manually deleted or rewritten.

`Set-WindowsSsdPaging.ps1` defaults to read-only inspection, has9 passing pure guard tests and checks the exact prior value, fixed-disk NTFS, target volume GUID,32GiB free-space floor and registry readback. It creates immutable HBP/HBI/SHA256 backups before the single registry-value change and rolls back on errors. Administrator rollback command: `./Set-WindowsSsdPaging.ps1 -Restore -Apply`; it restores the exact former D:8192..16384 setting, without rebooting. Run without switches to inspect. Corrected helper SHA256 `71769d94b321bcda9fecf874b6d637e0cb9e944c39d42bc30eaf177756e0b629`.

Backup/result under `windows-paging-receipts/`, stamp `20261020T223401257Z-c900c38e`, independently verified: before2 rows335 bytes, result1 row272 bytes; both SHA256 sidecars and full HBI byte ranges exact. The initial30-row office assessment also passed independent byte-range/hash verification. Correction retained beside the original: the first backup's informational `PAGINGINTENT.volume` field interpolated a CIM object display instead of its GUID. The actual volume-identity guard used the correct property and the paging target was correct. Fixed the helper's log interpolation, reran9 tests and idempotent inspection; no second settings write. Original receipts preserved.

### USB room expansion — protected, not allocated

The operator proposes additional rooms on the2TB USB device. Current Windows inspection reports GeneralUDisk, USB/removable,2097152000000 bytes at the disk layer. E: ASOESP FAT partition1015808 bytes is preserved. F: has a536870912000-byte partition but Windows reports filesystemUnknown and volumeSize0. That is a Windows-vantage boundary, not proof that native/raw data is absent, nor a verdict against the operator's2TB capacity. Verified writable capacity and random-I/O SSD performance have not been measured here.

No verified writable filesystem or owner-approved raw free region was established. **USB writes/format/repair/repartition/new rooms/capacity stress tests all0.** Additional rooms require a safe writable location and a tested writable memory-manager integration; they are not automatically available to arbitrary Windows programs. Existing D:12.5B rooms, spacing and243-extent placement map remain unchanged. No new resident service, Node process or Windows driver was installed.

Fabric USB request `council-q-1792535403925-oyp7tk`, HELM phase `council-q-1792535449401-l1pzr1`, directedUSB owners `council-q-1792535493955-5wcz0v`:ACK_ONLY, not owning ratification. Canonical law-text coverage15/15; that does not imply Windows-driver, behavior or speed verification. New sealed office packet `FABLE5-WINDOWS-SSD-PAGING-USB-HOLD-20261021.hbp`20 rows3202 bytes SHA256 `c95ce16b8991efae980361f86014173f391af866c6daa5305ba16a210eaf3059`; companionHBI22 rows1001 bytes SHA256 `001e46d1656db534b54a68f098d92feb005ad6def94a7a1799560525c9af0cf7`.

Microsoft describes ReadyBoost as a supplemental flash-storage cache, a separate mechanism from extending Windows paging or exposing Host8 to all processes: https://learn.microsoft.com/en-us/windows-hardware/customize/desktop/unattend/microsoft-windows-systemmaintenanceservice-cachelist-cache . Paging settings and current runtime state are explicitly distinct: https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-pagefilesetting . No measured trillions-fold or general ternary execution speedup is claimed.

## Historical assessment before approval — preserved below

2026-10-20, ACER-CLAUDE-FABLE5. This is a follow-up to the completed read-only Asolaria room adapters, not a claim that those adapters now back every Windows process.

## Current Windows state

- Installed physical RAM: 17,014,509,568 bytes; eight logical processors.
- Windows already has a system-wide pagefile, separate from Host8: `D:/pagefile.sys`, initial 8192 MiB / maximum 16384 MiB; automatic pagefile management disabled. Currently allocated8192 MiB, usage814 MiB before /819 MiB after sampling, peak1163 MiB since boot.
- C: is the internal NVMe SSD, free51,562,496,000 bytes at inspection. D: is the internal WD mechanical HDD, free349,304,561,664 bytes. USB disk2 was not read for data or modified.
- Six CIM performance samples across23,471 milliseconds: available physical memory7176–7242 MiB; committed memory11351–11429 MiB against24418 MiB limit, peak468 per-mille. CPU sample readings3–23 percent. Five memory samples had0 pages input/second, one63; all pages output/second readings0. D: had one busy interval, not sustained saturation. These observations do not diagnose other periods when the operator experiences slowness.
- Hard page reads include mapped files and program images, not exclusively pagefile reads; the counters do not establish that D: paging caused the busy interval.

## Proposed next change — NOT APPLIED

Move the existing Windows pagefile configuration from D: to `C:/pagefile.sys`, preserving its8192..16384 MiB bounds. This puts Windows paging on the SSD without reusing, truncating or relocating any Host8 room file. Keep approximately32 GiB of the inspected C: free space even at the configured16 GiB maximum, subject to other disk usage. Recheck free space and current configuration before applying. Operator choice and normal administrator approval are still needed; no automatic reboot.

This is an expectation of lower paging I/O latency, not a measured improvement or the diagnosis of the current slowdown. Windows paging backs eligible pageable allocations across processes; it is not extra physical DRAM, a replacement ternary execution engine, or a measured trillions-fold speedup. Direct Host8-backed writable Windows paging remains NOT_MEASURED in this work. Do not relabel the installed read-only Asolaria adapter as that missing measured integration.

## System grounding and boundaries

Fabric broad query `council-q-1792535044524-owxz5a`, directed Windows/RAM/watchdog query `council-q-1792535118448-kbnqkb`, and HELM/PROF-HELM/CEO query `council-q-1792535121151-y2c7xu` returned ACKs, not substantive owning-seat answers. Recent30 council verdict entries inspected; no matching answer observed. Recall queries returned0 entries, which is not system absence. Live Host8 unified-pipe route answered200. Windows SystemDriver name/path filter for Asolaria/Host8/Aether/ternary matched0; that limited filter is not a proof that no integration can exist under another name.

No paging registry/configuration writes, driver installs, process stops, room writes, layout changes, firmware changes or reboots. CrashDumpEnabled3 unchanged. `Get-MMAgent` access denied, so memory-compression state is NOT_MEASURED, not disabled.

## Corrections / diagnostic limitations

Sandbox CIM calls were denied; the outside-sandbox read succeeded. Do not report failed instrumentation as a failed Windows service. The first outside-sandbox sample command stopped at the administrator-only `Get-MMAgent` query before entering its counter loop; the successful six-sample loop omitted that query. Helper activity is included in the sampled workload. Process private commit is not resident physical RAM.

## Durable backend and primary references

Office `FABLE5-WINDOWS-WIDE-MEMORY-ASSESSMENT-20261020.hbp`:30 rows4595 bytes, SHA256 `bc7871e5410641c2412436ac2548bb43c75cf971d1740b18764ec628e908c6da`; companion HBI32 rows1473 bytes, SHA256 `079eb0948ddc508a5ac21be8a28e3ecb34df9df87a400b1e127ae807bead0cda`. Both independently sealed by the office API.

- https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/introduction-to-the-page-file
- https://learn.microsoft.com/en-us/troubleshoot/windows-client/performance/how-to-determine-the-appropriate-page-file-size-for-64-bit-versions-of-windows

The completed R3/on-metal/standalone integration remains documented in `ROOM-MEMORY-INTEGRATION.md`; this Windows proposal does not supersede or erase it.
