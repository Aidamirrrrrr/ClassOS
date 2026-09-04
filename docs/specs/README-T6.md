# ClassOS T6 — Policy Engine и Focus Mode

Статус по `docs/specs/BACKLOG.md`: **код завершён и покрыт автоматическими
проверками; ни одна строка Windows enforcement не выполнялась на реальной
машине.**

## Принятые границы реализации

Механизм enforcement выбран в
[ADR-0014](../architecture/adr/0014-t6-windows-enforcement-providers.md):
AppLocker для запуска приложений, policy-ключи реестра для системных
ограничений, enterprise-policy Chrome/Edge для URL. Assigned Access и WDAC в
T6 не используются.

Разделение слоёв повторяет уже принятый в T0 паттерн:

```text
policy-engine          продуктовая модель, слои, компилятор, safe rollout
agent-core::policy     состояние устройства, Focus, rollback, break-glass
agent-service          WindowsPolicyProvider: AppLocker + registry
windows-platform       примитивы: AppLocker XML/PowerShell, значения реестра
```

Продуктовый слой не содержит ни одного Windows-понятия, Teacher Console — тем
более (инвариант X).

## Реализовано

- слоистая модель Base → Branch → Room → Lesson → temporary Focus и
  детерминированный расчёт EffectivePolicy;
- Focus Mode **сужает** список приложений, а не дополняет нижние слои —
  иначе кнопка «разрешить только VS Code» не имела бы смысла;
- каталог приложений устройства: Teacher Console присылает `vscode`,
  устройство само разрешает идентификатор в `Code.exe`; неизвестный
  идентификатор — ошибка компиляции, а не молча пропущенное правило;
- обязательный auto-allow бинарников ClassOS при каждой компиляции;
- последовательность validate → snapshot → apply → verify → commit с
  гарантированным rollback при сбое на любом шаге после snapshot;
- снимок исходного состояния сохраняется на диск и переживает перезагрузку;
  повторное применение политики не затирает baseline;
- `check_support` до первого Apply: если AppLocker недоступен, политика **не
  применяется частично** и команда завершается `POLICY_UNSUPPORTED`;
- версионированный `PolicyDocument` в `ApplyPolicy.compiled_policy`: по сети
  идёт продуктовая политика, компиляция и auto-allow выполняются на
  устройстве, поэтому их нельзя обойти со стороны сети;
- маршрут Apply/Rollback/FocusEnable/FocusDisable в Service через тот же
  идемпотентный command envelope, что и T5;
- break-glass `classos-service.exe recover-policy`: только локально, с явной
  проверкой прав администратора, без сетевого маршрута;
- профили урока и Focus Mode в Teacher Console — кнопки `[Python] [Web]
  [Focus Mode]`, применяются ко всему классу одной командой.

## Автоматически проверяется

100 unit-тестов workspace, из них по T6:

- компилятор: auto-allow, разрешение идентификаторов в исполняемые файлы,
  отказ на приложении вне каталога, отказ на некорректном URL;
- layering нескольких уровней и циклы Focus enable/disable;
- rollback при сбое Verify, включая сбой при повторном применении поверх уже
  активной политики;
- недоступный enforcement не доходит до Apply и не оставляет изменений;
- break-glass восстанавливает состояние и идемпотентен;
- rollback отвергает чужой snapshot id;
- round-trip состояния на диске и версионирование сетевого документа;
- генерация AppLocker XML: allowlist без deny-правил, экранирование имён,
  детерминированные идентификаторы правил (выполняются только на Windows CI).

Также: `cargo fmt`, `cargo clippy -D warnings` и Windows cross-check
(`scripts/check-windows.sh`) на всём workspace, а с этого milestone CI
проверяет и Rust-backend Teacher Console — раньше он не собирался нигде.

## Не проверено на реальном оборудовании

Это самый большой разрыв T6: весь код, который реально трогает Windows,
существует только в скомпилированном виде.

- `Set-AppLockerPolicy` / `Test-AppLockerPolicy` / `Get-AppLockerPolicy` —
  ни один вызов не выполнялся; корректность генерируемого XML не подтверждена
  ни одним запуском AppLocker;
- запись и откат policy-ключей реестра;
- блокировка запуска приложения под standard-user — **главный пункт DoD
  (§2), не подтверждён**;
- поведение на редакции Windows без поддержки AppLocker и при остановленной
  службе Application Identity;
- break-glass на устройстве с активной политикой;
- Focus Mode на группе устройств в реальном классе.

До этих проверок T6 нельзя считать подтверждённым на целевой среде, и
заявлять «ClassOS перестал быть просто Veyon» тоже нельзя: enforcement
написан, но не наблюдался работающим.

## Известные ограничения

- Профили урока ограничены каталогом приложений устройства из T5
  (`vscode`, `chrome`, `python`). Расширение каталога — задача T7
  (software management), а не T6.
- Branch и Room — пустые слои по умолчанию; реальная иерархия появляется с
  Cloud v0 (T8). Модель слоёв реализована сразу целиком, чтобы её не
  переписывать.
- В варианте политики без allowlist (только запреты cmd/PowerShell) deny в
  AppLocker сильнее allow, поэтому ограничение действует и на локального
  администратора до отката. Путь восстановления — `recover-policy`, который
  разрешён всегда.
- URL-списки браузеров ограничены 16 записями: ровно этот диапазон
  охватывает snapshot, поэтому откат не может оставить хвост от более
  длинного предыдущего списка.
