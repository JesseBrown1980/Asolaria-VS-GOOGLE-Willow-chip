//! Safe bounded copies from a read-only Windows file mapping.
//! The shared no-std core retains forbid(unsafe_code). No mapped pointer or
//! borrowed slice is exposed by this crate.
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::float_arithmetic)]

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::ReadOnlyFile;
