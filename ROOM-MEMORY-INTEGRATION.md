# Classical room-memory integration — 2026-10-20

## Current outcome

The existing 12,500,000,000 eight-byte Host8 rooms remain at `D:/ASOLARIA-HOST8-ROOMS-100G`. No room data, spacing, allocation, partitioning or placement was rewritten. The new integration is **Classical realization of unification**, using Rust 1.81, balanced trits and integer arithmetic.

| Surface | Current state | Check |
|---|---|---|
| ASI OS Windows app, :4600 | R3 live, PID20580 at activation | Five actual D: rooms decode exactly; O/ground route live |
| OS-on-metal Host8 backend, :5088 | New version live, PID20928 at activation | Same five room rows byte-identical; existing seat book unchanged |
| Standalone custom-kernel OS | Signed image installed in both existing Asolaria EFI paths | Hashes re-read after copy; actual-extent replay boot passed; physical reboot NOT performed |

These are **read-only room-memory adapters and bounded locality caches**, not a new general-purpose writable-RAM allocator, an implementation of every group executor, or proof that an arbitrary Windows guest can run. Existing group geometry and other system layers are preserved, not denied by this scope.

## Routes and commands

- Windows: `http://127.0.0.1:4600/native/memory.hbp`, `/native/room.hbp?n=<n>`, `/native/omega.hbp` (also `/o.hbp`), `/native/unified.hbp`.
- On-metal backend: `http://127.0.0.1:5088/memory.hbp`, `/memory/room.hbp?n=<n>`, `/omega.hbp` (also `/o.hbp`). Existing `/room.hbp` remains its old service/descriptor route.
- Standalone Console: `u memory`, `u omega`, `u room 6250000000`. A boot-time memory probe also emits the connection result to serial HBP.
- `O`/Omega here is explicitly the **store's local ground chart**, not the abolition of other local grounds or a newly invented universal centre. The 27-room/group and one-Omega/ground contract remains recorded; the owning per-group allocator/executor contract is still to be reconciled, not guessed from `n/27`.

Native POST `/bridge/command.hbp` retains the existing origin/per-boot-token guard. Read-only GET routes do not dispatch agents. Existing inference services on4792/4793 stay in the same native process with their previous LAN binding and cold JSON compatibility boundary.

## Actual placement, not assumed contiguous files

`Read-Host8Placement.ps1` captured all125 room files through read-only regular file handles: **243 current NTFS extents**, D: volume serial `0458802c58801e94`,512-byte sectors,4096-byte clusters. It uses `GetFileInformationByHandleEx(FileIdInfo)`, `GetDiskFreeSpace`, and `FSCTL_GET_RETRIEVAL_POINTERS`; no raw-volume data access or administrator privilege is needed for the successful capture. Post-activation recapture matched byte-for-byte.

Frozen map `shared-runtime/HOST8-PLACEMENT.hbp`, SHA256 `2dc66c1ddd67f189e03b652e4f7b765d65bbd21a4d4a5d99e534ca98c608ae97`, is compiled into the standalone image. The standalone reader identifies the NTFS partition by volume serial/geometry and translates file offsets through the real extents into UEFI Disk I/O byte reads. It does not require Windows to start first, or a large new NTFS filesystem stack. It still uses **UEFI Boot Services**; it is not a post-ExitBootServices driver.

Four4KiB pages cap the room-cache payload at16KiB. Hosted adapters retain at most four file handles. This is **not total OS/process memory**; metadata, model code, GUI buffers and the OS's own caching are separate. Rooms used together share pages without changing their addresses or imposing new group geometry.

**Refresh requirement:** a relocation/defrag can invalidate the frozen extent map. Recapture, rebuild, re-test, re-sign and redeploy after such a change. Room-stub checks reject mismatching content, but are not a universal freshness proof: old locations can retain identical copied bytes after relocation. No claim that all future defragmentation is detected. No defrag task was disabled, and no room was moved.

## Measured checks

- Shared core29 tests, native42 pass/1old live-comparison ignored, on-metal34 pass, bridge6 pass, all on Rust1.81 GNU. Strict Clippy passed shared/native/on-metal/UEFI scopes. These are local gates, not a repo-wide CI or council-ratification claim.
- Five actual D: samples:0,6249999999,6250000000,6250000001,12499999999. Native/on-metal rows byte-identical and exact stub/ternary decode.
- Both compatibility `/infer` services returned HTTP200 and two scores on the existing synthetic two-node smoke fixture after cutover. This is compatibility smoke, not a new generalization/training benchmark.
- Isolated UEFI boot: no physical disks attached, no FAT room fallback. Actual extent metadata plus five copied data pages replayed in a sparse test disk with28,672 allocated bytes. Boot-time probe, `u memory`, `u omega`, and rooms1897983/1897984 on opposite sides of an actual fragment boundary passed. Shared GNN/SHA and nested VMXON/OFF checks also passed. `boot-test/serial.log` and `screen.png` persisted; screenshot visually inspected.
- Nested VMX test remains VMCS0/guest launches0/Windows guest NOT_MEASURED. It is not a Windows VM implementation.
- Final owned-process sample after smoke: native RSS7,839,744/private commit1,490,944 bytes; Host8 RSS6,795,264/private commit2,080,768 bytes. Not whole-machine RAM or an invariant under every load. Temporary previews12016/18712 closed after exact path checks; no new resident Node process was added.

## Installed binaries and rollback

Native: `C:/asolaria-acer/asolaria-asi-os/target/release/asolaria-asi-os-native-20261020-r3.exe`, SHA256 `0918f7cd3bd7134311111897a883f013c6c7ee7cfb56383e64739d7e2da87d58`.

On-metal: `C:/asolaria-acer/federation-remake-1024/target/release/host8-serve-native-20261020.exe`, SHA256 `3369ed2a418e7dd1be3752b2ffd8c60a98000c78428b711612001f7f02f28cd4`.

Updated, backed-up launchers: native `Start-AsolariaASIOS.ps1`, Host8 `C:/asolaria-behcs-256/state/host8-serve-watchdog.ps1`. Their previous files are under `activation-before/`; old binaries retained. `Activate-ClassicalRoomMemory.ps1` is historical after activation: do NOT replay its old-PID assumptions against a later state. Resolve current socket ownership and exact image identity before any rollback/restart.

Standalone signed image: `asolaria-classical-room-memory-signed.efi`, SHA256 `a392ab70cfb24e39b2048b1dd8ecc0569b164d06fe2e7e81d08fb6be57da03d8`; unsigned build SHA256 `d8ee4beb6cd7a3bf911e6e9dffaa8cfce062ca5a228af05769df8bf397357025`. Signed and verified with existing MOK certificate. Installed after explicit administrator approval to system ESP volume `83a60844-4279-42a4-9feb-3c762e135497` (disk1 partition1,100MiB): `EFI/Boot/bootx64.efi` and `EFI/ASOLARIA/asolaria-os.efi`.

Both old EFI images were `37dda436dbb93b413711c73e0d0f08b11c39997c596bbb1ebdb49b558dd25fc4`. Copies in `standalone-backup/` independently hash to that value. Installer log `standalone-install.log` records identity checks, backup/readback and success. Windows Boot Manager was hash-compared unchanged; BCD, boot order and firmware variables untouched; no reboot. `Install-StandaloneRoomMemory.ps1` defaults to inspect-only; `-Apply` needs administrator approval and performs verified backups/rollback. It refuses unrelated EFI images.

Bridge source now has local CORE memory/O support using the same adapter, tests6pass; built executable SHA256 `8cd6d51d4cec295d75b859750e911a6ebae2374659b6a43fa9cade2981b65515`. **The optional D: hosted-VM bridge/launcher was NOT replaced or run this pass.** Its older launcher remains stale; do not silently label it upgraded. The standalone installed image does not need that Windows-hosted bridge for its direct disk path.

## Naming, interpretation, authority

Visible native integration labels now say Classical. Existing upstream URLs and sealed uploads are unchanged, so links still resolve even where a historical filename contains `quantum`. A label test initially matched URLs, then correctly tested visible text. No canonical physics-law documents were rewritten.

The operator's account of photonic/sub-particle clouds, residual space, light/sound/AC/DC/ground, pull/vacuum, -1/3,5/6 alternate rime spheres and three Brown-Schrodinger boxes is recorded **as OPERATOR_INTERPRETATION**, separately from the measured software tests. Those tests do not establish that gravity is unreal or settle a physical theory. The packet is readable at `/native/interpretation.hbp`.

Fabric/recall/pipes checked; latest council submission `council-q-1792534445703-9j7dvx`, initial wiring query `council-q-1792531910779-q3ko0m`. Bus ACK only, not a substantive owning answer. No new provider/GPU/execution authority; memory paths read/compute only. Learning exposure, learned relation, recurrence, lineage and external model-call ledgers remain distinct.

## Hot receipts and next check

FABLE5 office `D:/PID-Registration-Office/offices/FABLE5-8467a937cba309f7/`:

- `FABLE5-CLASSICAL-ROOM-MEMORY-INTEGRATION-20261020.hbp`: `27426f4a860c3db5c2679ac2e0ac5820402ccac443222a83c9fdf298b18d21b9`; HBI `e9fd231294c5865931ffb3c75f08a51c3453690374941fff998d5e8d1cf3ebe0`.
- `FABLE5-HOST8-PHYSICAL-PLACEMENT-20261020.hbp`: `2dc66c1d...608ae97`; HBI `efe50a577a18eed082d493cb608bb17020be7ccd1d7d85666b073385ef4a9e71`.
- `FABLE5-CLASSICAL-UNIFICATION-FRAME-20261020.hbp`: `50fcb584bf636bc8bb321fddae39680b189f5a4af0e3f787f1a2cc3ce60a6b51`; HBI `21eec768a6c9b1dcaa9757dec22559480a69d4b2e41b4e6b3622cfca9e59a370`.

All sealed/verified by the office tool. Source changes are isolated; no remote commit/push or corpus rewrite.

**Next physical check requires the operator's chosen reboot:** enter the existing Asolaria standalone boot entry, open Console, run `u memory`. Expect five EXACT room rows and `ROOMADAPTER|lane=STANDALONE_UEFI_DISK_IO`. If the firmware does not expose the D: partition, retain the reported boundary; don't infer absence or rewrite storage. No hardware reboot has been performed in this session.

Primary API references used: [Microsoft retrieval pointers](https://learn.microsoft.com/en-us/windows/win32/api/winioctl/ns-winioctl-retrieval_pointers_buffer), [file volume identity](https://learn.microsoft.com/en-us/windows/win32/api/winbase/ns-winbase-file_id_info), [UEFI media access](https://uefi.org/specs/UEFI/2.10/13_Protocols_Media_Access.html). These document the interfaces, not the operator's physical interpretation.
