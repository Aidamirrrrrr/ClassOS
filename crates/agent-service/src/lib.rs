//! Переносимая библиотечная часть `agent-service`. Здесь находится разбор
//! CLI и проверка обновлений, которые можно тестировать без Windows.
//! Привилегированный runtime расположен в Windows-модулях бинарника согласно
//! `docs/specs/README-T0.md`.

pub mod cli;
pub mod update_checker;
