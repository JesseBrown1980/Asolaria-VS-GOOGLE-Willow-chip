# Host8 native mapping boundary

This zero-dependency Windows crate owns the small Win32 FFI boundary. The
shared runtime keeps `#![forbid(unsafe_code)]`; it consumes only the safe
`ReadOnlyFile::open` and `read_exact_at` methods. Pointers and borrowed mapped
slices never leave the owner.

`CreateFileMappingW(PAGE_READONLY)` and `MapViewOfFile(FILE_MAP_READ)` map the
existing file without extending, relocating or rewriting it. The file handle
allows read sharing and denies write/delete sharing until the view is dropped.
Drop unmaps the view before releasing mapping and file handles. Initialization
errors release partially acquired owners through normal Rust unwinding.

These are read-only deterministic room identities. They are not a writable
payload arena. Mapped virtual bytes are distinct from resident physical bytes;
Windows supplies page caching on demand. No prefault loop, VirtualLock or
private anonymous allocation is used.

Validation uses Rust 1.81, native Windows fixture tests, and clippy with warnings
denied. Fixture tests cover exact copies, bounds, share protection, cleanup and
moving an owned mapping to another thread. The shared-runtime example
`mapped_identity_probe` checks three identities in every production sector
without modifying a room.
