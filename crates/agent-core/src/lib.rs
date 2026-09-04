//! `agent-core`: независимая от ОС логика для service и session — ошибки,
//! конфигурация, доменные типы, trait'ы, supervisor и mock-реализации.
//!
//! Крейт собирается и тестируется на любой ОС без `windows-platform`.

pub mod commands;
pub mod config;
pub mod domain;
pub mod error;
pub mod mocks;
pub mod network;
pub mod remote;
pub mod stream;
pub mod supervisor;
pub mod traits;

pub use error::{AgentError, Result};
