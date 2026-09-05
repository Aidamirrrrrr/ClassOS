# ClassOS — Teacher Console

Приложение преподавателя: экраны класса, удалённое управление, режимы урока и
состояние компьютеров. Tauri 2 + React + TypeScript, backend на Rust
(`src-tauri`) переиспользует крейты агента `transport`, `agent-core`,
`protocol`, `policy-engine`.

Продуктовое имя — **ClassOS**; идентификатор пакета — `ru.classos.console`.
Имя крейта (`classos-console`) и имя npm-пакета (`@classos/teacher-console`)
внутренние и в интерфейсе не появляются.

## Разработка

```bash
pnpm install
pnpm tauri dev
```

## Сборка установщика

```bash
pnpm install --frozen-lockfile
pnpm exec tauri build
```

Результат — в `src-tauri/target/release/bundle`: на Windows `nsis` и `msi`, на
macOS `app` и `dmg`. Windows-установщик собирает и джоба `console-bundle` в CI,
складывая его в артефакты сборки: ошибки упаковки не видны ни `tsc`, ни
`cargo test` и проявляются только здесь.

Установщик пока **не подписан**. Authenticode нужен и агенту, и консоли — это
общий блокирующий пункт security-чеклиста T8 §10.

## Состояние на диске

`%APPDATA%\ClassOS\TeacherConsole` (переопределяется
`CLASSOS_TEACHER_STATE_DIR`):

- `teacher-authority.key` — ключ издателя. **Секрет:** он даёт подключение ко
  всем зарегистрированным устройствам, поэтому не должен попадать в резервные
  копии, доступные ученикам, и в репозиторий;
- `enrolled-devices.json` — реестр зарегистрированных устройств. Хранит
  подписанный credential, отпечаток сертификата и последний известный адрес.

Устройство, прошедшее enrollment, второй раз его не проходит, поэтому потеря
этих двух файлов означает обход класса руками
(`classos-service.exe reset-enrollment` на каждой машине). См.
[ADR-0018](../../docs/architecture/adr/0018-enrollment-window-and-credential-reissue.md).

## Что читать перед изменениями

`docs/architecture/01_TECHNICAL_ARCHITECTURE.md` §91–97 и спеку нужного
milestone. Интерфейс не показывает Windows-механизмы — ни AppLocker, ни
реестр, ни SID (инвариант X в `CLAUDE.md`).
