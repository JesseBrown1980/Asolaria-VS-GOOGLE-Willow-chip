# Native station dock

Rust 1.81, integer and ternary shared Host8 commands, HBP text only. The process
binds only 127.0.0.1 and contacts only its fixed loopback peer. Explicit ADB
forward/reverse tunnels provide the USB transport; this program never changes
ADB state. It runs at most 16 request workers and uses bounded headers, bodies
and socket timeouts.

Start each station with a different label, a local bind, the fixed local tunnel
port that reaches its peer, and a local file containing exactly 64 hexadecimal
characters (surrounding whitespace is allowed). Both stations use the same key.
The program never logs it. An optional --rooms path must name an existing
absolute directory of the original room sectors. A station without that path
reports room_source=NOT_MEASURED and may still query its peer.

All requests use HTTP/1.1 GET with a localhost or 127.0.0.1 Host authority.
Only /health.hbp is public. Other routes require Authorization: Bearer followed
by the exact 64-character key:

- /station.hbp: local station label, room configuration and local RAM telemetry.
- /dock.hbp: validate the peer schema and different station label, then compose
  both station reports.
- /memory/room.hbp?n=NUMBER: read one local existing deterministic identity.
- /peer/room.hbp?n=NUMBER: request one identity from the fixed peer.
- /task.hbp?cmd=ENCODED_COMMAND: execute one bounded pure shared-core command.
- /peer/task.hbp?cmd=ENCODED_COMMAND: execute it on the fixed peer.

Commands are status, sha HEX, gnn TWELVE_COMMA_SEPARATED_Q24_INTEGERS,
prism TWENTY_SEVEN_COMMA_SEPARATED_INTEGER_LANES, room NUMBER and memory.
Spaces and other URI-reserved characters must be percent encoded. Room and
memory commands need the local --rooms source. Other commands use the same
canonical integer/ternary implementation without room storage.

Peer routes first fetch the peer's local station report. They always invoke
the peer's local task route, never another peer route, so forwarding cannot
recurse. The station label is a routing label, not a newly minted device PID.
Existing device identities and 60D axes remain separate and preserved.
Room RPC is read-only access between independent address spaces. It does not
claim cache-coherent shared RAM, aggregate physical RAM or writable payload
allocation. Linux /proc/meminfo telemetry uses exact integer byte conversion;
other hosts report NOT_MEASURED.

No shell, arbitrary file route, network-selected destination, process-per-room
or room write exists. Local --rooms selects storage only at process startup.
Private replies and failures are HBP; JSON and Node/V8 are absent from the
runtime protocol. Timeout errors are observations; the server does not kill a
peer process.

Build/test with the explicit installed Rust 1.81 host toolchain. Tests exercise
authentication, HTTP boundaries, invalid commands, station-loop rejection and
real TCP execution of a shared-core SHA known answer. The shared core keeps its
own memory and integer/ternary tests.
