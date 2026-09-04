# ClassOS T6 — Policy Engine и Focus Mode

Статус: **собрана переносимая безопасная основа; Windows enforcement и реальная
приёмка ещё не выполнены.**

## Реализовано

- слоистая модель Base → Branch → Room → Lesson → temporary Focus;
- детерминированный расчёт EffectivePolicy;
- compiler с обязательным allow для ClassOS binaries;
- validate → snapshot → apply → verify → rollback, с unit-тестом rollback;
- protocol-варианты Apply/Rollback/Focus и локальная CLI-точка break-glass.

## До завершения T6

- Windows provider с реальным enforcement (не overlay);
- сохранение snapshot и локальная admin-проверка для ClassOS Recovery;
- подключение policy-команд к Service и UI Focus Mode;
- реальные тесты standard-user, blocked app, rollback и recovery.
