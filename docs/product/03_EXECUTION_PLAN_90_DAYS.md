# ClassOS — 90-Day Execution Plan

**Файл:** `docs/product/03_EXECUTION_PLAN_90_DAYS.md`
**Горизонт:** первые 90 дней
**Цель:** пройти путь от технического прототипа до работающего продукта в реальном компьютерном классе и первых платящих клиентов.

---

## 1. Главная цель 90 дней

Через 90 дней должно существовать не просто приложение и не красивое демо.

Должен существовать:

> **ClassOS, через который реально проводят занятия в компьютерной школе.**

Целевое состояние:

```text
3–5 реальных филиалов

50–150 управляемых Windows PC

10+ преподавателей

500+ проведённых classroom hours

1+ платящий клиент

Veyon отключён минимум в одном кабинете
```

Если это выполнено — продолжаем масштабировать продукт.

---

## 2. Главная гипотеза

Мы проверяем:

> Преподаватели и владельцы компьютерных школ готовы заменить Veyon на ClassOS, если ClassOS значительно удобнее управляет классом и автоматически ограничивает/подготавливает Windows-среду для занятия.

Пока эта гипотеза не подтверждена:

**AI — вторичен.**

**AlfaCRM — вторична.**

**аналитика — вторична.**

**enterprise — вторичен.**

---

## 3. Что НЕ является целью первых 90 дней

Не делаем:

* полноценную LMS;
* CRM;
* parent app;
* свою Windows shell;
* Linux;
* macOS;
* мобильные приложения;
* сложный ML;
* churn prediction;
* видеозапись занятий;
* собственный браузер;
* kernel drivers;
* собственный package manager;
* государственные тендеры;
* огромную cloud-инфраструктуру;
* идеальную multi-tenant enterprise architecture.

---

## 4. Product wedge

Первый ClassOS:

> **Veyon replacement + Windows classroom control.**

Основные функции:

```text
Live screens
Remote control
Lock
Message
Open App
Open URL
Restart
Shutdown

+

Focus Mode
Allowed Apps
Blocked Apps

+

Room management
Device health
```

Если этих функций недостаточно, чтобы преподаватель захотел использовать ClassOS вместо Veyon, дальнейшее развитие бессмысленно.

---

## 5. Основной пользователь

### Teacher

Первый UI проектируется исключительно вокруг преподавателя.

Не вокруг:

* директора;
* IT-администратора;
* родителя;
* владельца сети.

Первый вопрос продукта:

> Может ли преподаватель за несколько секунд понять состояние всего класса и выполнить нужное действие?

---

## 6. Второй пользователь

### Branch Administrator

Ему нужны:

```text
все ли ПК online
что на них установлено
какие проблемы
какие политики применены
```

Но полноценный IT-management появляется позже.

---

## 7. Product principles

### 7.1 Local first

Основные classroom-функции должны работать при отсутствии интернета.

```text
Teacher
↔
Student Agents
```

через локальную сеть.

---

### 7.2 Cloud optional for lesson

Cloud нужен для:

* auth;
* organizations;
* configuration;
* updates;
* analytics.

Но если cloud недоступен, занятие должно продолжаться.

---

### 7.3 Windows does enforcement

Не писать собственные security-механизмы без необходимости.

Используем Windows:

```text
AppLocker / App Control
Assigned Access
GPO/CSP
Firewall
WinGet
Windows Services
DXGI
SendInput
```

---

### 7.4 Zero bullshit UX

Не показываем преподавателю:

```text
GPO
CSP
AppLocker
SID
WMI
Win32
firewall rules
```

Он видит:

```text
[Python]
[Roblox]
[Focus]
[Open Chrome]
[Lock Class]
```

---

## 8. Architecture v0

```text
                 Teacher Console
                 Tauri + React
                       │
                       │ local protocol
                       │
        ┌──────────────┼──────────────┐
        │              │              │
        ▼              ▼              ▼

      PC-01          PC-02          PC-03

     Service        Service        Service
        │              │              │
 Session Host     Session Host     Session Host
        │              │              │
     Windows        Windows        Windows
```

Cloud:

```text
                 ClassOS Cloud
                      │
           ┌──────────┼──────────┐
           │          │          │
          Auth      Config      Logs
```

---

## 9. Proposed repository

```text
classos/
│
├── apps/
│   ├── teacher/
│   └── admin/
│
├── services/
│   └── cloud/
│
├── crates/
│   ├── agent-service/
│   ├── agent-session/
│   ├── agent-core/
│   ├── protocol/
│   ├── windows-platform/
│   └── common/
│
├── packages/
│   ├── ui/
│   ├── api-client/
│   └── shared/
│
├── installer/
│
├── docs/
│   ├── 01_ROADMAP.md
│   ├── 02_PRODUCT_ANALYSIS.md
│   └── 03_EXECUTION_PLAN_90_DAYS.md
│
└── README.md
```

---

## 10. Stack

### Windows Agent

```text
Rust
windows-rs
Tokio
```

---

### Teacher

```text
Tauri 2
React
TypeScript
```

---

### Cloud

```text
Bun
TypeScript
PostgreSQL
```

Redis:

**не нужен на первом этапе.**

---

## 11. Communication protocol

Не делать десятки случайных JSON WebSocket events.

Сразу описать protocol.

Минимальные messages:

```text
Hello
Authenticate

DeviceInfo
DeviceHealth
Heartbeat

ScreenSubscribe
ScreenFrame
ScreenUnsubscribe

RemoteControlStart
RemoteInput
RemoteControlStop

ExecuteCommand
CommandResult

PolicyApply
PolicyResult
```

---

## 12. WEEK 1

## Agent foundation

Главная цель:

> Student PC появляется в Teacher Console.

---

### Agent Service

Создать Windows Service.

Функции:

```text
install
start
stop
auto-start
machine ID
version
heartbeat
```

---

### Machine identity

Каждый ПК получает:

```text
deviceId
deviceName
hostname
Windows version
agent version
```

---

### Local connection

На MVP:

```text
Teacher Console
↔
Agent
```

по локальной сети.

---

### Teacher UI

Первый экран:

```text
Room

PC-01   ONLINE
PC-02   ONLINE
PC-03   OFFLINE
```

---

### Week 1 DoD

Две Windows-машины.

Teacher открывается.

Student автоматически появляется.

После reboot student снова появляется.

---

## 13. WEEK 2

## Screen capture

Реализовать:

```text
DXGI Desktop Duplication
```

---

### Первый этап

Не streaming.

Сначала:

```text
capture screenshot
↓
JPEG/WebP
↓
Teacher
```

---

### Затем thumbnails

```text
1 FPS
640x360 approximately
```

---

### Teacher UI

```text
┌──────────┬──────────┐
│ PC-01    │ PC-02    │
│ screen   │ screen   │
├──────────┼──────────┤
│ PC-03    │ PC-04    │
│ screen   │ screen   │
└──────────┴──────────┘
```

---

### Week 2 DoD

Teacher видит одновременно минимум:

```text
4 student screens
```

в реальном времени.

---

## 14. WEEK 3

## Fullscreen + Remote Control

При клике на устройство:

```text
PC-04

[ LIVE SCREEN ]

[ Take Control ]
```

---

### Streaming modes

#### Grid

```text
1–2 FPS
low quality
```

#### Selected

```text
15+ FPS target
higher quality
```

---

### Input

Реализовать:

```text
mouse move
left/right click
scroll
keyboard
special keys
```

через `SendInput`.

---

### Security

Remote control разрешён только после:

```text
authorized teacher
+
explicit session
```

---

### Student indication

На student PC:

```text
Teacher connected
```

---

### Week 3 DoD

Можно с Teacher PC:

```text
открыть Student
↓
управлять мышью
↓
печатать
↓
закрыть connection
```

---

## 15. CHECKPOINT #1

После 3 недель сделать первое демо реальному преподавателю.

Не продавать.

Показать.

Спросить:

1. Что в Veyon используешь каждый урок?
2. Чего здесь не хватает?
3. Что раздражает?
4. Какая кнопка должна быть на главном экране?
5. Что никогда не используешь в Veyon?
6. Как часто подключаешься к ученику?
7. Как часто блокируешь класс?
8. Как запускаете приложения/сайты?
9. Что ломается чаще всего?
10. Что дети пытаются обходить?

Все ответы записать.

---

## 16. WEEK 4

## Veyon feature parity

Добавить classroom actions.

```text
Lock
Unlock

Message

Open App
Open URL

Restart
Shutdown
```

---

### Multi-device actions

Обязательная часть.

```text
Select:
☑ PC-01
☑ PC-02
☑ PC-03

[Open VS Code]
```

или:

```text
[Select All]

[Lock]
```

---

### Teacher UX

Самые частые actions должны выполняться максимум за:

**1–2 клика.**

---

## 17. WEEK 5

## Real classroom pilot

Ставим ClassOS примерно на:

```text
10–20 PC
```

в одном кабинете.

---

### Veyon пока НЕ удаляем

Он остаётся fallback.

Но преподаватель должен пытаться использовать ClassOS.

---

### Собираем telemetry продукта

```text
teacher session started
device connected
screen opened
remote session
lock used
message sent
app launched
command failed
```

---

## 18. Pilot metrics

Измерять:

```text
agent uptime
device connection rate
stream start success
command success
remote-control success

teacher sessions
teacher actions
```

---

### Reliability targets

Минимально:

```text
Agent uptime > 99%

Command success > 98%

Screen connection > 95%

No classroom-blocking crash
```

---

## 19. WEEK 6

## Focus Mode v0

Начинается настоящее отличие от Veyon.

Первый вариант простой.

Teacher:

```text
Focus Mode

Allowed:

☑ VS Code
☑ Python

[Enable]
```

Student:

```text
VS Code ✓

Steam ✕
Discord ✕
Telegram ✕
other apps ✕
```

---

### Не делать идеальный security system

Сначала проверить UX и ценность.

---

### Student account

Тесты проводить под:

```text
standard Windows user
```

Не administrator.

---

## 20. WEEK 7

## Windows Policy Engine

Создать внутри agent abstraction:

```text
Policy
```

Например:

```json
{
  "allowedApps": [],
  "blockedApps": [],
  "settings": {},
  "browser": {},
  "network": {}
}
```

---

### Первый набор

```text
application restrictions

Settings restrictions

PowerShell restriction

cmd restriction

Microsoft Store restriction

personalization restriction
```

---

### Apply / rollback

Критично:

```text
Apply Policy
↓
Lesson
↓
Rollback
```

Если rollback ненадёжен — feature нельзя выпускать.

---

## 21. WEEK 8

## Lesson Profiles

Добавляем первую продуктовую abstraction.

Не:

```text
Policy #238
```

А:

```text
Python Class
Roblox Class
Design Class
```

---

### Python

```text
VS Code
Python
Chrome

Block:
Roblox
Steam
Discord
```

---

### Roblox

```text
Roblox Studio
Blender
Chrome
```

---

### Teacher UI

Главный экран:

```text
Today's environment

[ Python ]
[ Roblox ]
[ Design ]
```

Teacher:

```text
[Start Python]
```

---

## 22. CHECKPOINT #2

К этому моменту ClassOS должен уже отличаться от Veyon.

Проводим реальное занятие:

```text
Teacher enters
↓
opens ClassOS
↓
Start Python
↓
class configured
↓
lesson
↓
Finish
```

Спросить:

> Если завтра оставить только Veyon, чего тебе будет не хватать?

Если ответ:

> Focus / profiles / удобство ClassOS

— мы движемся правильно.

---

## 23. WEEK 9

## Device Health

Начинаем продавать продукт владельцу филиала.

Собираем:

```text
CPU
RAM
Disk
Windows version
Agent version
uptime
installed software
```

---

### Dashboard

```text
Room 1

12 devices

10 Healthy
1 Warning
1 Offline
```

---

### Warning examples

```text
Low disk
Agent outdated
App missing
Windows reboot required
```

---

## 24. WEEK 10

## Software inventory

Нужно понимать:

```text
что установлено
какая версия
```

Начать с важных приложений:

```text
VS Code
Python
Node
Git
Chrome
Roblox Studio
Unity
Blender
```

---

### Desired software

Первый software profile:

```text
Python Classroom

Python
VS Code
Git
Chrome
```

---

## 25. WEEK 11

## Deployment

Начать интеграцию с WinGet.

Admin:

```text
PC-07

Python missing

[Install]
```

---

Затем:

```text
Room 2

4 PCs missing Python

[Install on all]
```

---

### Не делать пока

Не делать полноценный SCCM/Intune.

Нам достаточно нескольких приложений, которые нужны нашим design partners.

---

## 26. Killer Demo к Week 11

Владелец видит:

```text
20 computers
```

Нажимаем:

```text
Python Mode
```

Компьютеры:

```text
VS Code ready
Python ready
wrong apps blocked
```

Потом:

```text
PC-07
Python missing
```

ClassOS:

```text
[Repair]
```

↓

Python устанавливается.

Вот это уже очень сильное B2B demo.

---

## 27. WEEK 12

## Commercial pilot

Теперь впервые просим деньги.

Не годовой контракт.

Предложение:

```text
ClassOS Pilot

1 classroom
up to 20 PCs
30 days

5 000–10 000 ₽
```

Конкретную цену тестируем.

Главное:

**человек должен достать карту/счёт и заплатить.**

---

## 28. Почему не бесплатно

Первый design partner может быть бесплатным.

Второй-третий уже должны платить хоть небольшую сумму.

Иначе невозможно понять:

> нравится продукт

или

> готовы покупать продукт.

---

## 29. WEEK 13

## Second branch

Цель:

не улучшать бесконечно первый филиал.

А установить систему во втором.

---

Почему это критично:

Первая установка может работать благодаря:

```text
нашему ручному шаманству
```

Вторая показывает:

> продукт воспроизводим.

---

## 30. Installation experience

К концу 90 дней:

на Student PC:

```text
ClassOSInstaller.exe
```

↓

```text
Branch code
```

↓

готово.

Не должно требоваться:

```text
открыть regedit
изменить firewall
запустить PowerShell
изменить 14 параметров
```

вручную.

---

## 31. Deployment target

Новый класс:

```text
20 PCs
```

должен быть подключён менее чем за:

```text
30–60 минут
```

без разработчика ClassOS на месте.

---

## 32. Cloud v0

К этому моменту добавить минимальную облачную сущность:

```text
Organization
Branch
Room
Device
User
```

---

### Roles

```text
Owner
Admin
Teacher
```

---

## 33. Teacher permissions

Teacher:

```text
view classroom
control classroom
apply lesson profile
```

Не:

```text
change organization policies
manage billing
install arbitrary privileged software
```

---

## 34. Owner permissions

Owner:

```text
branches
rooms
devices
teachers
profiles
health
billing
```

---

## 35. Security checklist до real rollout

Обязательно:

```text
signed installer
signed binaries

encrypted connections

device identity

authorization

audit remote sessions

secure auto-update

no plaintext secrets

standard-user student accounts
```

---

## 36. Auto-update

Agent update — обязательная инфраструктура.

Нельзя физически ходить по 100 компьютерам.

Нужна модель:

```text
ClassOS Cloud
↓
signed update manifest
↓
Service
↓
verify signature
↓
install
↓
rollback on failure
```

---

## 37. Crash resilience

Service должен переживать:

```text
Session Host crash
Teacher disconnect
Internet outage
User logout
Windows reboot
sleep/wake
network reconnect
```

---

## 38. Local discovery

В пределах школы Teacher Console должен уметь быстро находить ClassOS devices.

Но discovery никогда не должен автоматически давать control.

```text
Discovery
≠
Authentication
```

---

## 39. Первый sales script

Не продавать:

> AI-powered ClassOS platform.

Продавать:

> «Мы сделали современную систему управления компьютерным классом вместо Veyon. Преподаватель видит все ПК, может подключаться к детям, запускать программы и одним нажатием включать режим урока, в котором детям доступны только необходимые приложения.»

Потом показать demo.

---

## 40. Discovery questions владельцу

Перед установкой:

1. Сколько филиалов?
2. Сколько кабинетов?
3. Сколько ПК?
4. Какая Windows?
5. Есть ли локальные администраторы?
6. Используется ли Veyon?
7. Какие функции Veyon используют?
8. Какая CRM?
9. Как устанавливается ПО?
10. Кто обслуживает компьютеры?
11. Как часто ломается среда?
12. Какие программы нужны?
13. Что дети запускают лишнего?
14. Что преподаватели чаще всего просят исправить?
15. Сколько времени уходит на настройку класса?

---

## 41. Teacher interview

После недели использования:

1. Что открываешь первым?
2. Какие функции используешь?
3. Что мешает?
4. Где ClassOS хуже Veyon?
5. Где ClassOS лучше?
6. Сколько раз использовал Focus?
7. Сколько раз подключался удалённо?
8. Какие действия всё ещё делаешь вручную?
9. Что должно происходить автоматически?
10. Если ClassOS завтра исчезнет, расстроишься?

---

## 42. Product analytics events

Минимальный event schema:

```text
teacher_session_started

lesson_profile_started

lesson_profile_finished

device_connected

device_disconnected

screen_opened

remote_control_started

command_sent

command_failed

focus_enabled

focus_disabled

policy_failed

repair_started

repair_completed
```

---

## 43. North-star

### Managed Classroom Hours

Считается:

```text
active Teacher Session
+
connected classroom
+
lesson running
```

---

## 44. 90-day target metrics

### Product

```text
500+ managed classroom hours

50+ managed PCs

10+ weekly-active teachers
```

Минимум.

Хороший результат:

```text
1 000+ classroom hours

100+ PCs
```

---

### Reliability

```text
Agent uptime >99%

commands >98% successful

remote-control start >95%

policy apply >98%
```

---

### Business

```text
3–5 organizations piloting

1–3 paying

1 branch removed Veyon
```

---

## 45. Самая важная метрика

### Veyon Replacement Rate

Из кабинетов, где установлен ClassOS:

```text
сколько перестали использовать Veyon?
```

Если:

```text
0%
```

у нас проблема.

Если:

```text
50%+
```

есть сильный сигнал.

Если:

```text
100%
```

очень хороший сигнал.

---

## 46. Когда добавлять AlfaCRM

НЕ в первые недели.

Триггер:

> минимум 2 школы регулярно проводят занятия через ClassOS.

Тогда подключаем:

```text
schedule
group
teacher
students
```

---

## 47. AlfaCRM MVP

ClassOS открывается:

```text
Сегодня

16:00 Roblox
Teacher: Иван
10 students

18:00 Python
Teacher: Мария
12 students
```

Teacher нажимает:

```text
Start
```

и ClassOS автоматически выбирает Lesson Profile.

---

## 48. Student identity

После AlfaCRM:

```text
PC-07

[Кто ты?]

Артём
Миша
Катя
```

или PIN.

Получаем:

```text
Device
+
Student
+
Lesson
```

---

## 49. Когда добавлять AI

Триггер:

```text
1000+ real classroom hours
```

или хотя бы достаточно реальных sessions, чтобы понимать, какие сигналы полезны.

Не делать AI до понимания реального workflow.

---

## 50. AI Phase 1

Не LLM.

### Rules

Например:

```text
student active
+
IDE active
+
project unchanged 6 min
+
not idle
```

↓

```text
🟡 Possibly stuck
```

---

## 51. AI Phase 2

Добавить данные IDE/terminal.

Например:

```text
same compiler error
×
4
```

↓

```text
🔴 Needs help
```

---

## 52. AI Phase 3

LLM/VLM получает:

```text
lesson
task
code
error
recent activity
optional screenshot
```

и возвращает:

```text
probable problem
confidence
teacher recommendation
```

---

## 53. AI Supervisor UI

Teacher больше не должен смотреть постоянно на 12 screens.

Главный view постепенно становится:

```text
Python • 12 students

🟢 8 Working

🔴 1 Needs help
🟡 2 Possibly stuck
🔵 1 Finished
```

---

## 54. Что не делать с AI

Не делать:

```text
continuous video → GPT
```

дорого, медленно и плохо для privacy.

Использовать:

```text
events
↓
detector
↓
AI only when necessary
```

---

## 55. Когда поднимать инвестиции

Не в Day 1.

Не после prototype.

Первый момент для нормального разговора:

```text
5+ schools
100–300 endpoints
real usage
paying customers
strong retention
```

---

## 56. До инвестиций

Founder должен доказать:

```text
Problem
Product
Usage
Willingness to pay
```

Инвестор уже помогает масштабировать.

---

## 57. Funding roadmap

### Stage 0

Founder funded.

```text
0 → MVP
```

---

### Stage 1

Грант / небольшой angel.

После первых pilot/customer signals.

Использование:

```text
Windows engineer
security
product
installer/updater
```

---

### Stage 2

Pre-seed.

После:

```text
5–20 schools
hundreds of endpoints
repeatable onboarding
```

---

### Stage 3

Seed.

После:

```text
thousands of endpoints
repeatable sales
enterprise pipeline
```

---

## 58. Первый hire

Если founder закрывает:

```text
TS
backend
frontend
product
```

первый сильный сотрудник:

### Senior Windows Engineer

Нужны:

```text
Win32
DXGI
Windows Services
Sessions
Security
App Control
Networking
Installers
Updates
```

---

## 59. Второй hire

После первых клиентов:

### Product/Windows engineer

который помогает делать:

* Teacher UX;
* agent;
* deployment;
* reliability.

---

## 60. Первый sales hire

Не раньше repeatable sales motion.

До этого:

## Founder-led sales

Потому что первые 20 клиентов фактически помогают проектировать продукт.

---

## 61. Marketing первые 90 дней

Практически никакого paid marketing.

Основные каналы:

```text
личные контакты
бывшие коллеги
директора филиалов
франчайзи
Telegram сообщества
рефералы
демо
```

---

## 62. Public demo video

Обязательно сделать после Week 8–10.

60–90 секунд.

Сценарий:

```text
20 computers

↓
Start Python

↓
VS Code opens everywhere

↓
Steam blocked

↓
Teacher sees all screens

↓
Teacher connects to student

↓
Focus

↓
Finish
```

Конец:

## ClassOS

### Turn any computer into a learning computer

---

## 63. Landing page

Минимальная.

Не 20 страниц.

---

### Hero

## Весь компьютерный класс под контролем

Управляйте приложениями, экранами и учебной средой на всех Windows-компьютерах из одного места.

```text
[Посмотреть демо]
[Попробовать в школе]
```

---

## 64. Case study

После первого успешного филиала:

> Как KIBERone N заменил Veyon и автоматизировал подготовку компьютерного класса.

Метрики:

```text
devices
teachers
lessons
hours
technical incidents
```

Без выдуманных цифр.

---

## 65. Referral

После появления нескольких филиалов:

```text
Invite another branch
```

↓

оба получают:

```text
1 month ClassOS free
```

или другой incentive.

---

## 66. Go / No-Go decision — Day 90

### GO

Продолжаем серьёзно, если:

```text
3+ active pilots

1+ paying school

teachers use product weekly

ClassOS replaces Veyon somewhere

strong qualitative feedback

technical reliability acceptable
```

---

## 67. STRONG GO

Ускоряемся, если:

```text
5+ paying branches

teachers actively recommend ClassOS

other franchisees request access

Veyon removed from several classrooms

software management actively used
```

Тогда:

* AlfaCRM;
* AI Supervisor;
* senior hire;
* fundraising preparation.

---

## 68. PIVOT

Если teachers используют:

```text
remote screen
```

но игнорируют:

```text
Focus
Lesson Profiles
```

тогда ClassOS рискует стать просто Veyon clone.

Нужно искать другой wedge.

---

## 69. Возможный pivot №1

Если владельцам намного важнее:

```text
device management
```

чем teacher control:

строить:

> **IT management for education centers.**

---

## 70. Возможный pivot №2

Если больше всего ценится автоматическая подготовка урока:

строить:

> **Technical Lab Orchestration Platform.**

---

## 71. Возможный pivot №3

Если AI Supervisor показывает огромный эффект:

двигаться в:

> **AI classroom orchestration.**

---

## 72. KILL

Проект стоит серьёзно пересмотреть, если после:

```text
3 реальных школ
+
30+ преподавательских sessions
```

преподаватели всё равно предпочитают Veyon.

Это намного полезнее узнать через 90 дней, чем через два года.

---

## 73. Backlog P0

До первого пилота обязательно:

```text
Agent Service

Session Host

Device discovery

Authentication

Screen thumbnails

Full screen

Remote control

Lock

Message

Open app

Open URL

Restart

Shutdown

Logging

Auto reconnect
```

---

## 74. Backlog P1

После первого пилота:

```text
Focus

Allowed Apps

Blocked Apps

Lesson Profiles

Room management

Device health

Software inventory
```

---

## 75. Backlog P2

После подтверждения usage:

```text
Software deployment

Repair

Auto-update

Cloud organizations

RBAC

Policy inheritance
```

---

## 76. Backlog P3

После нескольких клиентов:

```text
AlfaCRM

Lesson Engine

Student identity

Telemetry
```

---

## 77. Backlog P4

После данных:

```text
Stuck detection

AI Supervisor

AI Tutor

Reports

Analytics
```

---

## 78. Основной engineering principle

Каждую новую функцию проверять вопросом:

> **Помогает ли она провести реальный урок лучше?**

Если нет — не сейчас.

---

## 79. Основной product principle

Не строить:

> самую мощную систему управления Windows.

Строить:

> **самый удобный способ провести компьютерный урок.**

---

## 80. Основной business principle

Первые клиенты — не revenue source.

Они:

```text
design partners
+
distribution seed
+
proof
```

Но после первых 1–2 design partners нужно обязательно проверить реальную оплату.

---

## 81. Главный milestone

Не:

> v1 released.

Не:

> AI works.

Не:

> 100 GitHub stars.

А:

## «В реальном KIBERone удалили Veyon, потому что ClassOS лучше.»

После этого у проекта действительно начинается компания.

---

## 82. 90-Day Timeline Summary

```text
W1
Agent + discovery

W2
Screen capture

W3
Remote control

────────────
DEMO #1
────────────

W4
Classroom commands

W5
Real pilot

W6
Focus Mode

W7
Windows policies

W8
Lesson Profiles

────────────
DEMO #2
────────────

W9
Device Health

W10
Software inventory

W11
Deploy / Repair

W12
Paid pilot

W13
Second/third branch
```

---

## 83. После 90 дней

Если гипотеза подтверждена:

```text
Veyon Replacement
        ✓
        ↓
Device Management
        ✓
        ↓
Lesson Engine
        ↓
AlfaCRM
        ↓
AI Supervisor
        ↓
AI Tutor
        ↓
Analytics
        ↓
Network / Enterprise
```

---

## 84. Итог

Первые 90 дней ClassOS — не попытка построить большую образовательную ОС.

Это попытка доказать одну простую вещь:

> **Можно сделать настолько хороший инструмент управления компьютерным классом, что преподаватель добровольно перестанет пользоваться Veyon.**

Если это доказано, у нас появляется:

* установленный agent на каждом компьютере;
* ежедневный пользователь — преподаватель;
* связь с реальным учебным процессом;
* канал для deployment;
* источник telemetry;
* возможность подключения AlfaCRM;
* фундамент AI Supervisor;
* база для enterprise-management.

И тогда ClassOS начинает превращаться из утилиты в:

## **Operating System for Computer Classrooms.**
