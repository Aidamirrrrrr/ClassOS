# ClassOS

## Product & Technical Roadmap v0.1

**Дата:** 3 сентября 2026
**Статус:** концепт → technical prototype

---

## 1. Что мы строим

**ClassOS — система управления компьютерными классами для IT-школ.**

Она должна заменить Veyon и одновременно дать школе то, чего Veyon вообще не пытается решать:

* централизованное управление Windows-компьютерами;
* контроль приложений и системных настроек;
* режимы занятий;
* удалённый просмотр и управление;
* автоматическую подготовку компьютеров;
* интеграцию с расписанием/учениками;
* AI-помощника преподавателя;
* AI-тьютора ученика;
* аналитику обучения;
* диагностику технических проблем;
* аналитику retention.

Первый целевой рынок:

**KIBERone / Алгоритмика / TOP / независимые школы программирования и компьютерные кружки.**

Первоначальный wedge:

> **Современная замена Veyon + управление Windows-классом.**

Большой продукт:

> **Operating system for computer classrooms.**

При этом физически компьютеры продолжают работать на обычной Windows.

---

## 2. Основная проблема

Сегодня компьютерный класс состоит примерно из:

```text
Windows
+
Veyon
+
AlfaCRM
+
браузер
+
куча установленного ПО
+
ручная работа преподавателя/администратора
```

Это несколько разрозненных систем.

Veyon знает компьютер, но не знает:

* ученика;
* урок;
* программу обучения;
* прогресс;
* задания;
* расписание;
* AlfaCRM;
* причины, почему ребёнок застрял.

AlfaCRM знает:

* ученика;
* преподавателя;
* группу;
* расписание;
* посещаемость;
* оплаты.

Но не знает, что фактически происходит на компьютере во время урока.

ClassOS должен соединить эти два мира.

---

## 3. Product vision

В идеальном состоянии преподаватель приходит в класс и нажимает:

### «Начать занятие»

После этого автоматически:

```text
AlfaCRM
↓
кто сегодня занимается

ClassOS
↓
какой курс и какой урок

Windows
↓
подготавливает рабочие места

ПК учеников
↓
разрешает необходимые приложения
↓
блокирует ненужные
↓
открывает материалы
↓
загружает проекты

Teacher Console
↓
показывает состояние класса
```

Во время урока:

```text
Артём     🟢 работает
Миша      🟡 возможно застрял
Катя      🔵 закончила раньше
Саша      🔴 нужна помощь
```

После урока:

```text
проекты сохранены
↓
приложения закрыты
↓
учебная среда очищена
↓
результаты сохранены
↓
AlfaCRM обновлена
↓
сформирован отчёт
```

---

## 4. Главный принцип архитектуры

### Не переписывать Windows

Наша система должна максимально использовать возможности самой Windows.

Microsoft уже предоставляет restricted user experience через Assigned Access; режим позволяет оставить знакомый Windows desktop, но ограничить список запускаемых приложений, Start/Taskbar и применить дополнительные политики/AppLocker rules. Microsoft прямо указывает student devices и lab devices как целевые сценарии. Assigned Access поддерживается в Windows Pro, Enterprise, Education и IoT Enterprise.

Следовательно:

```text
ClassOS
    ↓
оркестрация

Windows
    ↓
enforcement
```

Мы не строим:

* свой explorer;
* свой package manager;
* свой firewall;
* свой process manager;
* свой remote desktop driver;
* свою систему пользователей.

Мы строим хороший слой управления над существующими Windows API.

---

## 5. Общая архитектура

```text
                         CLASSOS CLOUD
┌────────────────────────────────────────────────────────┐
│                                                        │
│ API                                                    │
│ Auth                                                   │
│ Organizations                                          │
│ Policies                                               │
│ AlfaCRM Connector                                      │
│ Analytics                                              │
│ AI                                                     │
│ Updates                                                │
│                                                        │
└───────────────────────┬────────────────────────────────┘
                        │
                        │ HTTPS/WSS
                        │
                 SCHOOL / BRANCH
                        │
               ┌────────┴────────┐
               │ Teacher Console │
               └────────┬────────┘
                        │
                 local network
            ┌───────────┼───────────┐
            │           │           │
            ▼           ▼           ▼

          PC-01       PC-02       PC-03
        ┌───────┐   ┌───────┐   ┌───────┐
        │Agent  │   │Agent  │   │Agent  │
        └───┬───┘   └───┬───┘   └───┬───┘
            │           │           │
          Windows     Windows     Windows
```

Очень важно:

### управление классом должно работать без облака

Если интернет школы отвалился:

* live screens работают;
* remote control работает;
* Focus Mode работает;
* запуск приложений работает;
* блокировки работают;
* урок продолжается.

Cloud нужен для:

* аккаунтов;
* синхронизации;
* аналитики;
* AI;
* AlfaCRM;
* управления сетью филиалов.

Но не для базового classroom control.

---

## 6. Архитектура Student Agent

Нельзя делать один `agent.exe`.

Windows Services с Windows Vista изолированы в Session 0 и не должны напрямую взаимодействовать с пользовательским desktop. Microsoft рекомендует разделять service и процесс, работающий внутри пользовательской interactive session.

Поэтому:

```text
ClassOS Service
LocalSystem
│
│ IPC
│
└── ClassOS Session Host
    текущий пользователь
```

### ClassOS Service

Привилегированная часть.

Отвечает за:

* конфигурацию машины;
* политики;
* software installation;
* services;
* firewall;
* updates;
* Windows users;
* device enrollment;
* health;
* reboot/shutdown;
* communication с backend;
* запуск Session Host;
* защищённое обновление ClassOS.

Работает как Windows Service.

---

### ClassOS Session Host

Запускается внутри пользовательской сессии.

Отвечает за:

* screen capture;
* remote mouse/keyboard;
* active window;
* user idle state;
* overlay;
* student UI;
* уведомления;
* AI helper;
* session telemetry.

Это критическое разделение должно оставаться во всей архитектуре.

---

## 7. Remote screen

Для основного механизма захвата экрана использовать:

### DXGI Desktop Duplication API

Microsoft создавал Desktop Duplication именно для сценариев desktop collaboration и remote desktop; API отдаёт изображения рабочего стола из GPU memory плюс dirty regions, перемещения областей и состояние курсора, что позволяет эффективно передавать только изменения. Microsoft отдельно отмечает применение подобных систем в enterprise и education.

Архитектура:

```text
DXGI
↓
Desktop Duplication
↓
GPU frame
↓
encoder
↓
network
↓
Teacher Console
```

Не нужно постоянно передавать 30 FPS всех 15 компьютеров.

#### Grid mode

Для общего вида:

```text
1–2 FPS
низкое разрешение
```

#### Selected PC

После открытия конкретного ученика:

```text
15–30 FPS
высокое разрешение
```

Это радикально снижает network/CPU/GPU overhead.

`Windows.Graphics.Capture` также остаётся полезным для захвата конкретных окон; Windows предоставляет API для кадров display/application window.

---

## 8. Remote input

Для первого варианта remote control используем Win32 `SendInput`.

Windows умеет синтезировать:

* keyboard;
* mouse move;
* mouse buttons.

```text
Teacher
↓
mouse event
↓
ClassOS protocol
↓
Session Host
↓
SendInput
↓
Windows
```

Ограничение правильное и желательное: из-за UIPI такой механизм не должен управлять окнами с более высоким integrity level.

То есть ученик работает как **standard user**, а secure/admin UI остаётся вне remote-control.

---

## 9. Windows lockdown

Будет два уровня.

### Base Policy

Постоянные ограничения ученического компьютера.

Например:

```text
Student Base

No Settings
No Control Panel
No Store
No personalization
No user creation
No software installation
No PowerShell
No cmd
No registry tools
No admin
```

Используем:

* Assigned Access;
* CSP;
* GPO;
* AppLocker/App Control.

Windows имеет две штатные технологии application control: App Control for Business и AppLocker. AppLocker позволяет разрешать/запрещать приложения, scripts, installers и packaged apps; Microsoft рекомендует App Control для более серьёзных security-сценариев, а AppLocker остаётся полезным для управляемых environments.

---

## 10. Lesson Policy

Отдельно существует динамический профиль урока.

Например:

### Python

```text
ALLOW

VS Code
Python
Git
Chrome

WEB

docs.python.org
github.com
stackoverflow.com

BLOCK

Roblox
Unity
Steam
Discord
Telegram
```

### Roblox

```text
ALLOW

Roblox Studio
Chrome
Blender

WEB

create.roblox.com
school materials
```

### Design

```text
ALLOW

Figma
Chrome
Blender
```

Преподаватель не должен видеть Windows policies.

Он выбирает:

```text
[ Python ]
[ Roblox ]
[ Design ]
```

---

## 11. Focus Mode

Одна из killer features MVP.

Преподаватель:

> **Focus Mode**

ClassOS:

```text
оставляет текущее учебное приложение
↓
закрывает/блокирует лишнее
↓
ограничивает интернет
↓
показывает статус на всех ПК
```

Teacher Console:

```text
FOCUS MODE

12 / 12 computers protected

Allowed:
VS Code
Python

Internet:
course resources only
```

Можно иметь режимы:

```text
Normal
Work
Focus
Research
Exam
Presentation
```

---

## 12. Browser control

На первом этапе не писать собственный network filter.

Использовать enterprise policies браузеров:

* URL allowlist;
* URL blocklist;
* extensions;
* incognito;
* downloads;
* homepage/search settings.

Это надёжнее для HTTP/HTTPS, чем пытаться фильтровать домены обычным Windows Firewall.

На позднем этапе для системного network control можно использовать **Windows Filtering Platform** — Windows предоставляет WFP именно как API/system-service platform для приложений сетевой фильтрации.

---

## 13. Software management

Это огромная часть ценности.

Не писать package manager.

Использовать:

### WinGet

и позднее:

### WinGet Configuration + DSC

WinGet Configuration позволяет декларативно описать packages, tools, dependencies и настройки машины в YAML, после чего WinGet + PowerShell DSC приводят систему к desired state.

ClassOS profile:

```text
Python Classroom v3
```

фактически превращается в:

```text
Python 3.x
VS Code
Git
Chrome
VS Code extensions
required policies
course resources
```

Администратор нажимает:

> Deploy to Room 2

---

## 14. Configuration drift

Каждый компьютер должен иметь:

```text
Desired state

vs

Actual state
```

Например:

```text
Room 2

PC-01 ✅
PC-02 ✅
PC-03 ⚠ Python incorrect version
PC-04 ✅
PC-05 ⚠ VS Code extension missing
PC-06 🔴 agent offline
PC-07 ⚠ disk 93%
```

Кнопка:

> **Repair**

Это отдельная коммерчески сильная функция даже без AI.

---

## 15. Shared computers

Windows имеет **Shared PC mode**, специально рассчитанный на устройства, которыми пользуется много людей, включая school scenarios. Он уменьшает administrative overhead от множества локальных user profiles и поддерживает специальные настройки shared devices.

Это идеально соответствует IT-школе.

ClassOS должен уметь настроить устройство как:

```text
ClassOS Managed Shared PC
```

И автоматически управлять:

* локальными аккаунтами;
* временными данными;
* профилями;
* storage cleanup;
* sign-in;
* logout.

---

## 16. Teacher Console

Основной интерфейс преподавателя.

Главный экран:

```text
Python • группа 12
18:00–20:00

┌─────────┬─────────┬─────────┐
│ Артём   │ Катя    │ Миша    │
│ screen  │ screen  │ screen  │
│ 🟢      │ 🟢      │ 🟡      │
├─────────┼─────────┼─────────┤
│ Саша    │ Дима    │ Лиза    │
│ screen  │ screen  │ screen  │
│ 🔴      │ 🟢      │ 🟢      │
└─────────┴─────────┴─────────┘
```

Actions:

```text
Focus
Open app
Open URL
Send message
Lock
Restart
Shutdown
Broadcast
```

---

## 17. Device Detail

При клике:

```text
Миша
PC-07

[ LIVE SCREEN ]

Python        running
VS Code       active
Chrome        2 min ago

CPU           21%
RAM           62%
Disk          48%

Lesson
Task 4/7

[Remote Control]
[Message]
[Restart app]
```

Позже сюда добавляется AI.

---

## 18. Classroom actions

Veyon replacement должен как минимум иметь:

| Функция                  | MVP              |
| ------------------------ | ---------------- |
| Device discovery         | ✅               |
| Online/offline           | ✅               |
| Screen thumbnails        | ✅               |
| Full live screen         | ✅               |
| Remote control           | ✅               |
| Lock screen              | ✅               |
| Send message             | ✅               |
| Open app                 | ✅               |
| Open URL                 | ✅               |
| Restart                  | ✅               |
| Shutdown                 | ✅               |
| Focus Mode               | ✅               |
| App restrictions         | ✅               |
| Teacher screen broadcast | после MVP        |
| File transfer            | низкий приоритет |

Цель первого релиза:

> преподаватель должен иметь возможность удалить Veyon и не захотеть установить его обратно.

---

## 19. Process management

Для приложений, запущенных ClassOS, использовать Windows **Job Objects**.

Job Object позволяет управлять группой процессов как единым объектом, включая ресурсные ограничения и завершение связанных процессов.

Например:

```text
Lesson #1482
│
├── VS Code
├── Python
├── child process
└── terminal
```

Finish Lesson:

```text
Terminate Job
```

Всё дерево корректно завершается.

---

## 20. Telemetry

Не нужно постоянно анализировать экран AI.

Сначала собираем дешёвые структурированные сигналы.

```text
active application
active window
idle time
process started
process stopped
CPU
RAM
disk
network state
file/project activity
lesson status
teacher interventions
AI requests
```

Windows ETW позволяет получать kernel/application events в реальном времени и динамически включать tracing без перезапуска приложения или компьютера.

Но ETW использовать постепенно.

MVP может начинаться с обычного Process/WMI/Win32 monitoring.

---

## 21. AlfaCRM integration

ClassOS **не заменяет AlfaCRM**.

AlfaCRM остаётся source of truth для:

```text
students
groups
teachers
lessons
schedule
attendance
payments
```

ClassOS становится source of truth для:

```text
devices
sessions
software
policies
class activity
interventions
AI activity
learning telemetry
```

---

## 22. AlfaCRM technical integration

У AlfaCRM есть REST API v2. Их актуальная документация рекомендует использовать единый API-клиент/rate limiter; документирует общий лимит **5 RPS**, кеширование, пагинацию и retries.

Следовательно:

```text
ClassOS
    │
    └── AlfaCRM Connector
            │
            ├── one rate limiter
            ├── cache
            ├── sync
            └── retries
```

Не разрешать каждому worker обращаться к AlfaCRM самостоятельно.

AlfaCRM также умеет через свои triggers отправлять webhook-запросы по событиям клиентов, групп, уроков, посещаемости, платежей и другим сущностям.

Поэтому:

```text
AlfaCRM webhook
↓
ClassOS
↓
event processing
```

лучше постоянного polling там, где webhook доступен.

---

## 23. Student identity

Не обязательно создавать отдельный Windows account каждому ребёнку.

Можно иметь:

```text
Windows:
student-room-1
```

и внутри ClassOS Session:

```text
Кто сегодня работает?

Артём
Миша
Катя
...
```

Список приходит из AlfaCRM.

Ребёнок:

```text
выбирает себя
+
PIN
```

После этого:

```text
Device Session
=
Device
+
Student
+
Lesson
+
Teacher
```

Это становится фундаментальной сущностью аналитики.

---

## 24. Lesson Engine

ClassOS должен иметь абстракцию:

```text
LessonSession
```

Пример:

```text
lessonId
branchId
roomId
teacherId
courseId
groupId

startTime
endTime

students[]
devices[]

softwareProfile
policyProfile
webProfile

status
```

Lifecycle:

```text
Scheduled
↓
Preparing
↓
Ready
↓
Running
↓
Finishing
↓
Completed
```

---

## 25. Start Lesson

Команда:

> Start Python Lesson

запускает workflow:

```text
1. Проверить компьютеры

2. Проверить software profile

3. Применить lesson policy

4. Закрыть приложения прошлого занятия

5. Запустить VS Code

6. Открыть проекты

7. Активировать monitoring

8. Создать DeviceSession

9. Teacher Console → LIVE
```

---

## 26. Finish Lesson

```text
1. Save project state

2. Stop telemetry

3. Close lesson apps

4. Remove temporary permissions

5. Sync session data

6. Restore default profile

7. Update AlfaCRM

8. Generate summary
```

---

## 27. AI architecture

AI появляется **не в MVP ядра**.

Он добавляется только после того, как есть качественные события.

Главный принцип:

> AI получает контекст, а не просто screenshot.

Контекст:

```text
student
age

course
lesson
task

active application
active file
recent events

previous difficulties
teacher interventions

optional screenshot
```

---

## 28. AI Supervisor

Первая AI-фича.

Не чат.

Не генерация уроков.

А:

### «Кому преподавателю подойти сейчас?»

Например:

```text
12 учеников

AI PRIORITY

🔴 Миша
6:31 без прогресса
одинаковая ошибка повторилась 4 раза

🟡 Полина
Task 3 значительно дольше медианы

🔵 Катя
закончила все основные задания
```

Это реально увеличивает возможности одного преподавателя.

---

## 29. Stuck detection v1

AI даже не нужен.

Rule-based:

```text
active lesson
+
IDE open
+
project changed == false
+
idle == false
+
same application
+
> X minutes
```

↓

```text
Possibly stuck
```

Это надо сделать раньше LLM.

---

## 30. Stuck detection v2

Добавляем:

* compiler errors;
* terminal output;
* project state;
* repeated commands;
* screenshot;
* lesson task.

AI определяет:

```text
Problem:
incorrect loop condition

Confidence:
87%

Teacher suggestion:
ask the student what condition
terminates the loop.
```

---

## 31. AI Tutor

Student нажимает:

> 🤖 Помоги

AI уже знает:

```text
курс
урок
задание
код
уровень ребёнка
```

Политика AI:

```text
не выдавать решение сразу

↓
подсказка

↓
вопрос

↓
ещё одна подсказка

↓
пример

↓
готовое решение только если разрешено
```

Учитель может выбирать:

```text
AI OFF
Hints only
Normal
Free
```

---

## 32. Screen Vision

Скриншоты не должны постоянно уходить в cloud.

Предпочтительная модель:

```text
local telemetry
↓
detector
↓
подозрение на проблему
↓
single screenshot
↓
AI analysis
```

То есть AI Vision вызывается **по событию**.

Плюсы:

* дешевле;
* меньше traffic;
* меньше privacy risk;
* меньше latency;
* легче продавать школам.

По умолчанию raw screenshots вообще не должны сохраняться после анализа.

---

## 33. Parent Report

Когда данные уже накоплены:

```text
Артём

Сегодня:
Loops / Python

Выполнено:
6/7

Самостоятельно:
5 задач

Подсказки:
2

Помощь преподавателя:
1

Основная сложность:
nested conditions
```

AI переводит техническую телеметрию в понятный родителю язык.

Потом это можно отправлять через существующую инфраструктуру школы.

---

## 34. Retention Intelligence

Это поздний этап.

Соединяем:

```text
AlfaCRM

attendance
payments
course
group
status
```

с:

```text
ClassOS

engagement
progress
difficulty
interventions
AI usage
```

И получаем:

```text
Risk of churn

Миша Иванов

72%

Signals:

↓ engagement
↑ difficulties
2 absences
↓ progress
```

Это уже функция для директора, а не преподавателя.

---

## 35. Admin Console

Отдельный web-интерфейс.

```text
Organization
├── Branches
│   ├── Moscow North
│   │    ├── Room 1
│   │    └── Room 2
│   └── Moscow South
│
├── Devices
├── Software
├── Policies
├── Courses
├── Integrations
├── AI
└── Analytics
```

---

## 36. Device Management

```text
Room 3

PC-01    Online    Healthy
PC-02    Online    Healthy
PC-03    Offline
PC-04    Python mismatch
PC-05    Disk warning
```

Действия:

```text
Repair
Restart
Shutdown
Update
Move room
Apply profile
Reinstall package
Open logs
```

---

## 37. Software Profiles

Пример:

```text
Python Development

Python       3.x
VS Code      latest-approved
Git
Chrome

VS Code extensions:
Python
Pylance
```

Важно:

не всегда использовать `latest`.

Школе нужна:

> **approved version**

иначе обновление Python/Unity/Roblox посреди учебной программы может всё сломать.

---

## 38. Policy inheritance

Enterprise:

```text
KIBERone HQ
     ↓
Base Policy

     ↓
Region

     ↓
Branch

     ↓
Room

     ↓
Lesson
```

Например HQ запрещает:

```text
Steam
Torrent
unknown executables
```

Филиал не может это отменить.

Но может дополнительно запретить Discord.

---

## 39. Основные сущности БД

```text
Organization
Branch
Room

Device
DeviceHealth
DevicePolicy
SoftwareProfile

Teacher
Student
Group
Course
Lesson

LessonSession
DeviceSession

TelemetryEvent
Intervention
AIInteraction

Integration
IntegrationMapping
AuditEvent
```

---

## 40. Security model

Это критическая часть продукта.

Student Agent имеет высокие privileges.

Следовательно:

### Service

LocalSystem.

#### Session Host

Standard user.

#### Teacher

Не получает Windows Administrator credentials.

#### Communication

mTLS/device certificates.

#### Enrollment

одноразовый enrollment token.

#### Remote control

только авторизованный teacher/admin.

#### Commands

логируются.

#### Updates

только signed builds.

#### Credentials

никаких API-key в plaintext config.

---

## 41. Remote-access auditing

Любой remote connection:

```text
Teacher
Device
timestamp
lesson
duration
reason/action
```

должен попадать в AuditLog.

Student UI должен иметь явный индикатор:

```text
Teacher connected
```

Не делать скрытое remote surveillance.

---

## 42. Privacy by design

Поскольку продукт используется детьми:

**по умолчанию не хранить video/screenshots.**

Храним события:

```text
active application
duration
lesson progress
technical metrics
```

Raw frames существуют только для streaming.

AI snapshot:

```text
capture
↓
analysis
↓
delete
```

Долговременное хранение должно включаться отдельно.

Юридический/privacy workstream для конкретной страны запуска необходимо проводить отдельно до production deployment.

---

## 43. Proposed stack

### Student Agent

#### Rust

Почему:

* Windows API через `windows-rs`;
* хороший memory safety;
* маленький runtime footprint;
* удобно делать Windows Service;
* networking;
* native performance;
* DXGI/Win32;
* позже можно использовать часть кода в Teacher Console.

Структура:

```text
agent-service
agent-session
agent-core
agent-protocol
windows-platform
```

---

### Teacher Console

#### Tauri 2 + React + TypeScript

UI:

React/TS.

Native:

Rust.

Плюс:

можно переиспользовать `agent-protocol`.

---

### Cloud

Можно спокойно оставить знакомый стек:

```text
TypeScript
Bun
PostgreSQL
Redis — только когда понадобится
WebSocket
REST
```

На старте Redis необязателен.

---

### Protocol

Не привязывать architecture непосредственно к WebSocket messages.

Определить собственный versioned protocol:

```text
DeviceHello
DeviceStatus
ScreenFrame
RemoteInput
Command
CommandResult
Telemetry
LessonState
```

Wire:

```text
protobuf
```

или аналогичный schema-first формат.

---

## 44. ROADMAP

---

## PHASE 0 — Windows Technical Spike

### Цель

Доказать, что фундамент работает.

Две машины:

```text
Teacher
Student
```

Реализовать:

* Windows Service;
* Session Host;
* IPC Service ↔ Session Host;
* enrollment;
* device ID;
* heartbeat;
* DXGI screenshot;
* stream thumbnails;
* remote mouse;
* remote keyboard;
* process list;
* launch process;
* restart/shutdown.

#### Definition of Done

Teacher machine:

```text
видит Student PC
↓
видит экран
↓
управляет мышью
↓
может запустить приложение
↓
может перезагрузить PC
```

После reboot Agent автоматически возвращается online.

Никакого cloud.

Никакого AI.

Никакой AlfaCRM.

---

## PHASE 1 — Veyon Replacement MVP

### Цель

Убрать Veyon в одном реальном кабинете.

#### Teacher Console

```text
rooms
devices
screen grid
fullscreen view
remote control
```

#### Actions

```text
Lock
Message
Open app
Open URL
Restart
Shutdown
```

#### Infrastructure

```text
auto reconnect
local discovery
manual enrollment
encrypted transport
logs
agent auto-start
```

#### Definition of Done

Преподаватель может провести обычное занятие только через ClassOS.

---

## PHASE 2 — Classroom Control

Теперь продукт становится больше Veyon.

Добавить:

```text
Focus Mode
Allowed Applications
Blocked Applications
Settings restrictions
Browser restrictions
Lesson Profiles
Room Profiles
```

Пример:

```text
[Start Python Environment]
```

и весь кабинет автоматически настраивается.

### Definition of Done

На ученическом аккаунте невозможно:

* запустить запрещённое приложение;
* установить программу;
* открыть запрещённые Settings;
* использовать запрещённые учебным профилем программы.

Teacher может изменить режим всей группы одной командой.

---

## PHASE 3 — Device Management

Добавляем:

```text
software inventory
hardware inventory
health
disk
versions
updates
WinGet
software profiles
configuration drift
repair
```

Teacher/admin видит:

```text
22 / 24 Healthy
```

и может исправить проблемы централизованно.

### Definition of Done

Новый компьютер можно подключить к ClassOS и привести к стандарту кабинета практически без ручной настройки.

---

## PHASE 4 — Lesson Engine

Добавить бизнес-понятие:

```text
lesson
course
group
student
```

Пока без AlfaCRM можно создать их вручную.

Функции:

```text
Start Lesson
Finish Lesson
Student login/PIN
Lesson Profile
Project Workspace
Lesson telemetry
```

ClassOS перестаёт видеть только компьютеры.

Он начинает видеть **учебный процесс**.

---

## PHASE 5 — AlfaCRM

Connector:

```text
Students
Teachers
Groups
Lessons
Schedule
Attendance
```

Inbound:

```text
REST sync
Webhooks
```

Outbound:

```text
attendance
lesson result
comments/derived data where appropriate
```

Расписание появляется автоматически.

Teacher открывает ClassOS:

```text
18:00

Python 10–12
12 students

[Start]
```

---

## PHASE 6 — AI Supervisor

Первый AI.

Сначала rule engine:

```text
idle
no changes
errors
time
progress
```

Потом LLM/VLM.

Teacher Console:

```text
Needs attention

1. Миша
2. Катя
3. Саша
```

При открытии:

```text
What probably happened

What student is doing

Suggested teacher action
```

### Definition of Done

Преподаватель реально использует priority list вместо постоянного просмотра 12 экранов.

---

## PHASE 7 — AI Tutor

Добавить student assistant.

Контекст:

```text
course
lesson
task
project
history
```

Modes:

```text
Off
Hints
Guided
Normal
```

Teacher получает статистику AI usage.

---

## PHASE 8 — Reports & Learning Analytics

Teacher:

```text
lesson report
student progress
difficult topics
```

Parent:

```text
plain-language report
```

Director:

```text
group health
course health
teacher load
attendance
progress
```

---

## PHASE 9 — Retention Engine

Объединяем ClassOS + AlfaCRM.

Появляются:

```text
risk indicators
engagement trends
difficulty trends
absence trends
```

Не начинать с ML-модели.

Сначала:

```text
rules
+
statistics
```

Когда накопятся реальные данные — prediction model.

---

## PHASE 10 — Enterprise

Для KIBERone HQ / TOP / больших сетей:

```text
multi-branch
central policies
RBAC
audit logs
SSO
policy inheritance
bulk enrollment
bulk deployment
local relay/cache
self-hosted option
API
integration SDK
central analytics
fleet health
```

---

## 45. Что НЕ делать в первой версии

Не делать:

```text
свой Explorer
свою ОС
свой антивирус
kernel driver
свой package manager
свой браузер
сложный WFP filter
continuous AI video analysis
parent mobile app
CRM
billing
full LMS
churn ML
macOS
Linux
```

Это всё отвлекает от wedge.

---

## 46. Реальный MVP

Если максимально отрезать всё лишнее, первая коммерчески проверяемая версия:

```text
CLASSOS MVP

1. Windows Agent

2. Teacher Console

3. Live screen grid

4. Remote Control

5. Launch app / URL

6. Lock / Message / Restart / Shutdown

7. Focus Mode

8. Allowed applications

9. Basic device health

10. Room management
```

Всё.

---

## 47. Что должно вызвать «вау» у владельца школы

Не AI.

А демо:

```text
20 компьютеров
↓
Teacher Console
```

Нажимаем:

> **Python Mode**

Все компьютеры:

```text
VS Code opens
Python ready
Chrome restricted

Steam unavailable
Discord unavailable
Settings unavailable
```

Teacher видит 20 экранов.

Открывает ребёнка.

Управляет его мышью.

Нажимает:

> **Finish lesson**

Всё закрывается и возвращается в исходное состояние.

Вот это уже продаваемо.

---

## 48. AI появляется после этого демо

Второе демо:

```text
12 students
```

Teacher видит не просто экраны:

```text
🟢 Working
🟢 Working
🔴 Needs help
🟡 Possibly stuck
🔵 Finished
```

И тогда сообщение становится:

> Veyon показывает компьютеры.
>
> **ClassOS понимает класс.**

---

## 49. Основные продуктовые KPI

### Classroom reliability

```text
agent uptime
command success rate
stream start success
reconnect success
```

#### Teacher UX

```text
time to start lesson
actions per lesson
remote interventions
```

#### Fleet

```text
healthy devices %
configuration drift
failed software installs
```

#### Education

```text
task completion
interventions
stuck events
AI usage
```

#### Business

```text
active schools
active rooms
managed devices
managed lessons
MRR
retention
```

---

## 50. North-star metric

На раннем этапе:

> **Managed classroom hours per week.**

Не количество зарегистрированных преподавателей.

Не количество AI requests.

Не количество компьютеров в базе.

А количество часов, когда реальное занятие действительно проводится через ClassOS.

---

## 51. Commercial packaging

Можно продавать по количеству устройств.

Например логика тарифов:

```text
ClassOS Core
Remote control + classroom

ClassOS Manage
+ policies
+ software deployment
+ device management

ClassOS AI
+ AI Supervisor
+ AI Tutor
+ reports

ClassOS Enterprise
+ multiple branches
+ HQ policies
+ SSO
+ integrations
+ private deployment
```

Не делать AI обязательным для core-продукта.

---

## 52. Главный moat

На старте moat почти отсутствует.

Это нормально.

Потом он формируется слоями:

```text
Windows management
+
excellent classroom UX
+
lesson engine
+
AlfaCRM
+
learning telemetry
+
AI
+
historical data
+
network integrations
```

Особенно ценными становятся accumulated learning/operational data.

Veyon может скопировать кнопку Focus.

Но гораздо сложнее скопировать:

```text
student
+
course
+
lesson
+
device
+
activity
+
progress
+
CRM history
```

---

## 53. Конечное состояние продукта

Teacher:

> «Начать урок».

Student:

> просто работает.

Admin:

> все компьютеры исправны.

Director:

> видит эффективность классов.

Parent:

> понимает прогресс ребёнка.

AI:

> следит за состоянием обучения и помогает там, где человек действительно нужен.

Windows:

> выполняет всю низкоуровневую грязную работу.

AlfaCRM:

> остаётся CRM.

ClassOS:

> связывает всё вместе.

---

## Итоговая последовательность

```text
Windows technical spike
        ↓
Veyon replacement
        ↓
Focus / policies
        ↓
Device management
        ↓
Lesson Engine
        ↓
AlfaCRM
        ↓
AI Supervisor
        ↓
AI Tutor
        ↓
Learning Analytics
        ↓
Retention
        ↓
Enterprise fleet management
```

Самое важное правило проекта:

> **Не начинать с AI.**

Сначала сделать настолько хороший classroom-control продукт, чтобы в одной реальной школе можно было удалить Veyon.

После этого у нас появляется канал данных, устройство внутри класса и ежедневное использование преподавателями.

И только поверх этого AI превращается из красивого gimmick в реальное конкурентное преимущество.
