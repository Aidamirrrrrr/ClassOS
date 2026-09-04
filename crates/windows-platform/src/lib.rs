#![cfg(windows)]

//! `windows-platform`: весь raw unsafe Win32 FFI для T0. В бизнес-крейтах
//! низкоуровневых небезопасных вызовов Win32 быть не должно.
//!
//! Крейт предоставляет только Windows-примитивы без продуктовых сущностей
//! и собирается для target `x86_64-pc-windows-msvc`.

pub mod crypto;
pub mod error;
pub mod handles;
pub mod input;
pub mod pipes;
pub mod process;
pub mod security;
pub mod sessions;

pub use error::{PlatformError, Result};
