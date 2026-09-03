# ClassOS — полный продуктовый и инвестиционный анализ

**Дата:** 3 сентября 2026
**Стадия:** Concept / pre-MVP
**Рынок входа:** частные IT-школы и компьютерные классы
**Долгосрочный рынок:** K-12 / дополнительное образование / колледжи / учебные центры / corporate training

---

## 1. Executive Summary

ClassOS — программная платформа для управления компьютерными классами.

Первоначально:

> **Современная замена Veyon для Windows-классов.**

Но это только wedge.

Полное видение:

> **ClassOS превращает обычный компьютерный класс в управляемую образовательную среду, которая знает, какой урок сейчас идёт, какие программы нужны ученикам, чем они занимаются и кому требуется помощь.**

Система объединяет:

* classroom management;
* remote desktop;
* Windows device management;
* application control;
* Focus Mode;
* software deployment;
* автоматическую подготовку класса;
* расписание;
* AlfaCRM;
* student identity;
* lesson context;
* learning telemetry;
* AI Supervisor;
* AI Tutor;
* аналитику обучения;
* аналитику оборудования;
* multi-branch management.

---

## 2. Моя оценка проекта

| Показатель                   | Оценка |
| ---------------------------- | -----: |
| Реальность проблемы          |   9/10 |
| Простота объяснения продукта |   9/10 |
| Возможность сделать MVP      |   8/10 |
| Готовность B2B платить       |   7/10 |
| Конкуренция                  |   8/10 |
| Первоначальный moat          |   4/10 |
| Потенциальный moat           |   9/10 |
| Россия как стартовый рынок   |   9/10 |
| Россия как конечный рынок    |   6/10 |
| Международный потенциал      |   9/10 |
| Bootstrap-потенциал          |   9/10 |
| Венчурный потенциал          |   8/10 |

### Общая оценка: **8.5/10**

Но при одном условии:

**нельзя остановиться на “Veyon, только красивее”.**

Тогда получится хороший небольшой B2B-продукт.

Большая компания появляется, когда ClassOS становится:

> **lesson-aware device & classroom operating layer.**

---

## 3. Проблема действительно существует

Это не выдуманный pain.

OECD сообщает, что около **30% учеников регулярно отвлекаются на цифровые устройства во время уроков**. Ученики, сообщавшие о регулярном отвлечении, в среднем показывали результат по математике на 15 баллов ниже после корректировки социально-экономических факторов. OECD отдельно отмечает, что учителю трудно одновременно контролировать, чем каждый ученик занимается на устройстве.

И проблема настолько заметна глобально, что к марту 2026 года уже 114 образовательных систем ввели национальные ограничения на мобильные телефоны в школах.

Для IT-школ проблема ещё сильнее.

Ребёнку необходимо дать:

* Windows;
* Chrome;
* VS Code;
* Roblox Studio;
* Unity;
* Blender;
* Python;
* интернет.

То есть нельзя решить проблему:

> «Заберём устройства».

Компьютер здесь и есть инструмент обучения.

Следовательно, нужен:

> **управляемый компьютер.**

---

## 4. Кто клиент

Важно разделить пользователя и покупателя.

### Пользователь №1 — преподаватель

Его проблемы:

* 10–15 компьютеров одновременно;
* кто-то открыл YouTube;
* кто-то застрял;
* где-то не работает Python;
* один ученик потерял файл;
* другой закончил раньше;
* третий установил что-нибудь;
* приходится постоянно бегать между ПК.

Ему ClassOS продаётся через:

> **«Весь класс под контролем с одного экрана».**

---

### Пользователь №2 — системный администратор

Его pain другой:

```text
PC-3 → Python сломан
PC-7 → Unity старая
PC-9 → диск C забит
PC-12 → Roblox не запускается
PC-14 → кто-то изменил Windows
```

Ему продаётся:

> **«Все компьютеры всегда находятся в одинаковом рабочем состоянии».**

---

### Economic buyer №1 — владелец филиала

Он думает про:

* зарплаты;
* retention;
* качество занятий;
* количество учеников на преподавателя;
* downtime;
* стоимость администрирования.

Ему ClassOS нужно продавать как:

> **«Стандартизируйте проведение занятий и сократите технические проблемы».**

---

### Economic buyer №2 — центральный офис сети

Например KIBERone HQ.

Его интересует:

```text
1 000+ площадок
↓
единые policies
↓
единая конфигурация
↓
единый стандарт проведения уроков
↓
аналитика
```

И здесь стоимость продукта возрастает на порядок.

---

## 5. Почему KIBERone — идеальный первый рынок

KIBERone сейчас заявляет:

* 120 000+ резидентов;
* 36 стран;
* 1700+ КИБЕРшкол;
* 2000+ IT-специалистов.

Алгоритмика заявляет:

* 450 городов;
* 1,1 млн выпускников;
* присутствие в десятках стран.

TOP сейчас заявляет 300+ городов и 900 000+ студентов; на отдельных страницах — 500+ филиалов.

То есть рынок уже состоит из **огромных распределённых сетей физических образовательных точек**.

А это именно тот тип бизнеса, которому очень нужен centralized endpoint management.

---

## 6. Почему AlfaCRM — плюс, а не конкурент

ClassOS не должен становиться CRM.

AlfaCRM уже умеет:

* учеников;
* преподавателей;
* занятия;
* группы;
* расписание;
* платежи;
* attendance.

Более того, у неё официальный REST API v2 с нормальной интеграционной моделью и webhooks. API ограничен 5 запросами в секунду, что тоже нормально для нашей архитектуры через централизованный connector.

Следовательно:

```text
AlfaCRM
↓
WHO + WHEN

ClassOS
↓
WHAT HAPPENS DURING LESSON
```

Это идеальное разделение ответственности.

---

## 7. Конкуренты

И вот здесь важно не обманывать себя.

Рынок **далеко не пустой**.

---

### Veyon

Бесплатный open-source продукт.

Умеет:

* смотреть экраны;
* remote control;
* broadcast;
* блокировать компьютеры;
* отправлять файлы;
* сообщения;
* запускать программы;
* открывать сайты;
* reboot/shutdown.

Veyon заявляет более 100 000 загрузок в год.

#### Преимущество

**Бесплатный.**

Это серьёзнейший конкурент.

#### Недостаток

Он воспринимает устройство как:

> PC-12.

А не:

> Артём → Python → урок 14 → задача 6.

---

## 8. NetSupport School

Очень серьёзный конкурент.

Существует десятилетиями и кроме classroom management имеет отдельную Tech Console.

Она уже умеет:

* hardware inventory;
* software inventory;
* process management;
* application monitoring;
* internet monitoring;
* policies;
* remote support;
* power management.

То есть идея:

> «Veyon + device management»

сама по себе **не новая**.

Это принципиально важно.

---

## 9. LanSchool

Принадлежит Lenovo.

Есть:

* locally hosted версия;
* cloud;
* classroom management;
* device control;
* channel sales.

Lenovo даже включает LanSchool в bundle с некоторыми образовательными устройствами.

Это показывает ещё один потенциальный канал ClassOS:

> **OEM / поставщики компьютерных классов.**

---

## 10. Senso.cloud

На мой взгляд, **самый опасный конкурент нашей большой концепции**.

Senso уже объединяет:

* classroom management;
* screen monitoring;
* device management;
* network management;
* asset management;
* filtering;
* remote support;
* student safety.

Компания заявляет использование в 10 000 школах по всему миру.

То есть:

> classroom + MDM

уже валидирован как категория.

---

## 11. GoGuardian

Это доказательство того, насколько большой может стать категория.

Сегодня GoGuardian заявляет:

* 25M+ учащихся;
* 2M+ educators;
* обработку 2,4 млрд browsing events ежедневно.

GoGuardian Teacher уже умеет:

* live screens;
* tab management;
* website policies;
* blocking;
* Focus;
* communication;
* classroom sessions.

В 2021 Tiger Global инвестировал в GoGuardian **$200 млн**, оценив компанию более чем в **$1 млрд**.

Это очень важный precedent.

**Категория способна производить unicorn-компании.**

---

## 12. Где тогда наше окно

Нельзя позиционироваться как:

> classroom monitoring.

Красный океан.

Нельзя:

> MDM for schools.

Тоже красный океан.

Нельзя:

> AI for education.

Тем более.

Наш wedge:

## **Lesson-aware computer classroom**

То есть устройство знает:

```text
Student
+
Teacher
+
Course
+
Lesson
+
Task
+
Apps
+
Policies
+
Project
```

И соответственно само меняет своё состояние.

---

## 13. Главное отличие ClassOS

Сегодня продукты работают примерно так:

```text
Device
↓
policy
```

ClassOS:

```text
Schedule
↓
Lesson
↓
Students
↓
Devices
↓
Environment
```

Например:

### 15:00 Roblox

Автоматически:

```text
Roblox Studio     ON
Blender           ON
Chrome            LIMITED

Python            OFF
Steam             OFF
Discord           OFF
```

#### 17:00 Python

```text
VS Code           ON
Python            ON
Git               ON

Roblox            OFF
Unity             OFF
```

Это уже не обычный MDM.

---

## 14. Второе отличие — software environment management

Обычной школе достаточно:

> Word + браузер.

IT-школе нужны:

* Python;
* Node;
* Bun;
* Unity;
* Unreal;
* Roblox;
* Blender;
* VS Code;
* extensions;
* SDK;
* JDK;
* Git;
* библиотеки.

И версии имеют значение.

Поэтому ClassOS может быть особенно сильным именно для **technical education**.

---

## 15. Третье отличие — AI понимает не экран, а обучение

Обычный classroom AI:

> ребёнок находится не на том сайте.

ClassOS:

> ребёнок уже 7 минут пытается решить задачу с циклом; compiler возвращает одну и ту же ошибку; проект не изменялся 5 минут.

Это фундаментально более полезный сигнал.

---

## 16. Киллер-фича №1

Не AI.

### **Start Lesson**

Преподаватель нажимает:

```text
Start Python Lesson
```

Через несколько секунд:

```text
12/12 PCs ready

✓ VS Code
✓ Python
✓ projects
✓ browser policy
✓ lesson resources
✓ students

Focus policy active
```

Вот это должно продавать продукт.

---

## 17. Киллер-фича №2

### **Repair Classroom**

```text
PC-1 ✓
PC-2 ✓
PC-3 ⚠ Python mismatch
PC-4 ✓
PC-5 ⚠ extension missing
PC-6 ⚠ disk full

[Repair all]
```

Для сетей это может оказаться коммерчески ценнее remote screen.

---

## 18. Киллер-фича №3

### AI Supervisor

Не:

> «ChatGPT внутри школы».

А:

```text
WHO NEEDS TEACHER NOW?

1. Максим 🔴
7 min stuck

2. Полина 🟡
repeated compiler error

3. Катя 🔵
finished early
```

Одному преподавателю становится проще управлять 10–15 детьми.

---

## 19. Российский TAM — сначала узкий

Если брать только:

* KIBERone;
* Алгоритмику;
* TOP;
* аналогичные частные сети;

рынок уже интересный, но не гигантский.

Для масштаба: три названных бренда публично указывают суммарно тысячи филиалов/городов присутствия, хотя эти показатели нельзя буквально складывать — компании считают footprint по-разному.

Предположим чисто как модель:

```text
2 500 locations
×
20 PCs
=
50 000 devices
```

При:

```text
500 ₽ / device / month
```

это: **25 млн ₽ MRR**

или:

**300 млн ₽ ARR.**

При 30 устройствах:

**450 млн ₽ ARR.**

Это не прогноз.

Это просто показывает порядок величины niche.

---

## 20. Российский TAM после расширения

В России функционирует около **40 000 школ**, где учится свыше 18 млн детей.

Также в стране порядка **4 000 колледжей**.

Если даже предположить:

```text
10 000 образовательных организаций

×
20 managed PCs

×
500 ₽

×
12 месяцев
```

получаем:

## ~1,2 млрд ₽ ARR

И это без:

* вузов;
* частных школ;
* кружков;
* центров дополнительного образования;
* корпоративного обучения.

Но это сценарный расчёт, а не утверждение, что сейчас 10 тыс. организаций готовы купить ClassOS.

---

## 21. Проблема государственного рынка

Я бы туда **не лез первым**.

Причины:

* тендеры;
* долгие продажи;
* сертификация;
* российский реестр ПО;
* Linux;
* локальные требования;
* интеграторы.

И интересный момент: «Альт Образование» прямо поставляется с возможностью использования Veyon, а в августе 2026 BaseALT улучшил его поддержку в Wayland.

Поэтому Windows-only ClassOS плохо подходит как конечная стратегия государственного образования.

---

## 22. Но здесь есть второй огромный рынок

После PMF:

```text
ClassOS Windows
↓
ClassOS Linux Agent
↓
ALT Linux
↓
Russian public education
```

Тогда появляется возможность попасть в российский реестр ПО.

С 2025–2026 годов национальный режим при государственных закупках дополнительно усиливает преимущества/ограничения вокруг российского ПО.

Это может стать отдельным moat на российском рынке.

---

## 23. Global TAM значительно интереснее

Глобальный рынок уже подтверждён существующими компаниями.

Senso:

> 10 000 schools.

GoGuardian:

> 25M+ students.

Impero/NetSupport/LanSchool также работают международно.

Следовательно, при глобальной стратегии потенциальный рынок — **миллионы управляемых устройств**.

Например:

```text
1 000 000 devices
×
$3/month
=
$3M MRR

=
$36M ARR
```

При $5:

**$60M ARR.**

Вот здесь уже начинается нормальная venture-scale история.

---

## 24. Поэтому правильная стратегия рынка

Не:

```text
Россия → KIBERone → всё
```

А:

```text
Private IT schools
↓
EdTech networks
↓
Private education
↓
Colleges / labs
↓
General K-12
↓
Global market
```

---

## 25. Pricing

Я бы не конкурировал с Veyon ценой.

Нельзя выиграть ценовую войну с:

> free.

Нужно продавать **экономический результат**.

Предлагаемая структура:

### Core

Цена: **399 ₽ / устройство / месяц**

* live screens;
* remote control;
* lock;
* messages;
* launch;
* URLs;
* basic Focus.

---

### Manage

Цена: **699 ₽ / устройство / месяц**

Core +

* app policies;
* software deployment;
* health;
* profiles;
* classroom repair;
* lesson environments.

---

### Intelligence

Цена: **999 ₽ / устройство / месяц**

Manage +

* lesson telemetry;
* AI Supervisor;
* AI Tutor;
* reports;
* AlfaCRM.

Цены — гипотеза для тестирования, не готовый прайс.

---

## 26. Минимальный чек

Я бы сделал minimum branch subscription.

Например:

```text
7 900–9 900 ₽ / месяц
```

иначе школы с 5 компьютерами будут создавать непропорционально много поддержки.

---

## 27. Enterprise

Для сети:

```text
500+ devices
```

индивидуальная цена.

Можно добавить:

* HQ console;
* hierarchy;
* policies;
* audit;
* SLA;
* private cloud;
* on-premise;
* SSO;
* API;
* custom integrations.

---

## 28. Unit economics

У Core прекрасная потенциальная экономика.

Screen streaming остаётся локальным:

```text
Student ↔ Teacher
```

Cloud получает только:

* status;
* telemetry;
* config.

Следовательно, cloud cost на устройство очень маленький.

Gross margin Core потенциально может быть **очень высокой**.

Основной расход:

> support.

И поэтому deployment UX — одна из важнейших частей бизнеса.

---

## 29. AI economics

AI меняет economics.

Но даже здесь не надо анализировать видео постоянно.

Используем:

```text
telemetry
↓
rule engine
↓
suspicious event
↓
AI
```

Это может снизить inference cost в десятки раз относительно постоянного Vision.

AI можно поэтому продавать отдельным tier.

---

## 30. Первый GTM

Здесь у проекта есть огромное преимущество:

**есть очень понятный дизайн-партнёр — реальный филиал IT-школы.**

Первые продажи нельзя начинать рекламой.

Нужно:

```text
1 branch
20 computers
1 teacher
real lessons
```

---

## 31. Pilot #1

Предложение:

> «Дайте поставить ClassOS на один класс на месяц. Veyon пока оставим как fallback. Если преподаватели предпочтут ClassOS — продолжим.»

Цель:

не заработать.

Цель:

### удалить Veyon

---

## 32. Главный PMF-тест

Через месяц спросить:

> «Если завтра мы отключим ClassOS и вернём только Veyon — насколько это будет больно?»

Если ответ:

> «Да пофиг».

Закрываем/переделываем продукт.

Если:

> «Не трогайте, пожалуйста».

Есть сигнал PMF.

---

## 33. Первые 5 клиентов

После первого филиала:

```text
1 KIBERone
↓
ещё 3–5 франчайзи KIBERone
```

Это особенно удобно, потому что между владельцами франшиз существует коммуникация.

Не нужно сразу идти в HQ.

---

## 34. Почему не HQ сначала

Большая сеть потребует:

* security review;
* документацию;
* SLA;
* стабильность;
* onboarding;
* procurement;
* интеграции.

А один франчайзи может сказать:

> «Ну давайте попробуем на 20 компьютерах».

---

## 35. Потом идём в HQ

И разговор уже другой.

Не:

> «У нас есть идея».

А:

> «ClassOS уже работает в 12 ваших филиалах, управляет 300 компьютерами и через него проведено 6 400 учебных часов.»

Совсем другое переговорное положение.

---

## 36. Что продавать HQ

Не remote desktop.

HQ не волнует remote mouse.

Продажа:

### Standardization

```text
HQ
↓
Python Profile v7
↓
1 000 branches
↓
all classrooms identical
```

Вот здесь network contract может стать очень крупным.

---

## 37. Вирусность внутри франшизы

Очень хороший механизм distribution:

```text
franchisee A
↓
показывает ClassOS
↓
franchisee B
↓
franchisee C
```

Особенно если есть:

> Invite another branch.

Можно даже дать:

> месяц бесплатно за подключённый филиал.

---

## 38. Второй канал — преподаватели

Нужен красивый public demo.

Например:

### ClassOS Free Lab

До 5 компьютеров бесплатно.

Преподаватель может поставить дома/на небольшой кружок.

Это создаёт bottom-up adoption.

Но free tier я бы добавлял только после того, как установка станет практически беспроблемной.

---

## 39. Третий канал — интеграторы

В России огромное количество компаний продаёт:

> «компьютерный класс под ключ».

ПК + роутер + мебель + экран + ПО.

ClassOS можно продавать как часть такого комплекта.

Позже:

```text
PC manufacturer
+
ClassOS
```

И здесь есть precedent: Lenovo использует LanSchool как часть education hardware offering.

---

## 40. Четвёртый канал — EdTech ecosystem

Очень интересен текущий акселератор Сколково + «Просвещение», объявленный в июне 2026 года. «Просвещение» выступает корпоративным заказчиком и предоставляет пилотные площадки образовательным технологиям.

Для ClassOS это буквально целевой формат.

Но идти туда стоит уже с working prototype.

---

## 41. Маркетинговое сообщение

Не:

> Innovative AI-powered educational device orchestration platform.

Никто не понимает.

---

Для преподавателя:

## «Весь компьютерный класс под контролем.»

---

Для IT:

## «30 компьютеров. Одна конфигурация.»

---

Для директора:

## «Каждое занятие начинается с готового класса.»

---

Большой брендовый слоган:

## **Turn any computer into a learning computer.**

---

## 42. Как показывать продукт

ClassOS продаётся **демонстрацией**, а не лендингом.

Видео:

```text
20 Windows PCs
```

Teacher:

> Start Python

Все 20:

```text
VS Code opens
↓
projects load
↓
policies activate
```

Student пытается:

> Steam

```text
Blocked by ClassOS
```

Teacher:

> Focus

20 компьютеров блокируют лишнее.

Teacher открывает экран ученика.

Remote control.

Потом:

> Finish Lesson.

Готово.

Такое видео может продавать продукт за 60 секунд.

---

## 43. Контент-маркетинг

Лучший контент:

> «Как мы управляем 30 Windows-компьютерами без системного администратора».
>
> «Почему мы удалили Veyon из компьютерной школы».
>
> «Как подготовить 20 компьютеров к Python за 10 секунд».
>
> «Как запретить детям Steam, но оставить Python».

Это гораздо сильнее обычного:

> «5 преимуществ цифровизации образования».

---

## 44. Product roadmap с точки зрения бизнеса

### Stage A

#### Veyon killer

Цель:

> usage.

Никакого AI.

---

### Stage B

#### Device management

Цель:

> ROI директору.

Теперь ClassOS экономит технические часы.

---

### Stage C

#### Lesson Engine

Цель:

> differentiation.

Вот здесь начинает формироваться категория.

---

### Stage D

#### AlfaCRM

Цель:

> lock-in + automation.

---

### Stage E

#### AI Supervisor

Цель:

> teacher productivity.

---

### Stage F

#### AI Tutor

Цель:

> learning outcome.

---

### Stage G

#### Analytics

Цель:

> management value.

---

### Stage H

#### Network OS

Цель:

> enterprise ACV.

---

## 45. Когда появляется moat

На версии Veyon replacement:

**почти никакого.**

Кто угодно может повторить.

После Device Management:

небольшой.

После Lesson Engine:

интереснее.

После CRM:

значительно лучше.

После AI + данных:

очень хороший.

---

## 46. Data moat

Через два года ClassOS потенциально знает:

```text
1 000 000 lessons

↓
course
task
age
errors
time
difficulty
teacher interventions
AI interventions
outcomes
```

И появляется крайне интересный dataset:

> **как на самом деле дети осваивают программирование.**

Это может быть намного ценнее remote control technology.

---

## 47. Новое направление №1

### Curriculum Intelligence

ClassOS обнаруживает:

> Lesson 24 вызывает проблемы у 43% учеников.

Методист видит:

```text
Task 5

median completion:
18 min

expected:
8 min

stuck rate:
47%
```

То есть ClassOS начинает улучшать **сами учебные программы**.

Для Алгоритмики это уже очень сильный value proposition.

---

## 48. Новое направление №2

### Teacher Intelligence

Не рейтинг:

> плохой/хороший преподаватель.

А:

```text
Teacher A

average intervention response:
1:24

students stuck:
low

lesson pace:
normal
```

Можно находить перегруженные группы и слабые curriculum points.

---

## 49. Новое направление №3

### Automatic Labs

Можно уйти за пределы детского образования.

Например:

университет:

> Cybersecurity Lab.

корпоративное обучение:

> Data Science Lab.

Bootcamp:

> React environment.

Компания:

> Employee onboarding lab.

То есть Lesson Environment превращается в:

> **managed technical training environment.**

Это значительно расширяет рынок.

---

## 50. Новое направление №4

### Exams

Режим:

```text
EXAM

only IDE
no AI
no internet
no clipboard
no external applications
```

Может стать самостоятельным продуктом.

---

## 51. Новое направление №5

### Cloud Labs

Очень долгосрочно физические ПК можно дополнить:

```text
ClassOS
↓
Cloud VM
```

Ученик получает готовую временную машину.

Особенно:

* cybersecurity;
* Linux;
* databases;
* ML;
* DevOps.

Тогда ClassOS становится ещё шире.

---

## 52. Инвестиционная стратегия

Самое важное:

## сейчас деньги поднимать не надо

Ты технический founder.

Текущая стоимость проверки идеи очень маленькая.

Нам сначала нужно доказать:

```text
Teacher prefers ClassOS to Veyon
```

Для этого инвестиции не нужны.

---

## 53. Pre-seed stage

### До привлечения

должны быть:

* working agent;
* Teacher Console;
* 10–30 устройств;
* реальный филиал;
* реальные уроки;
* первые отзывы.

И желательно:

> первый платящий клиент.

---

## 54. Первый капитал

Я бы рассматривал три источника.

### №1 Собственные средства

Лучший источник на MVP.

Нет dilution.

---

#### №2 Гранты

Фонд содействия инновациям имеет программу «Старт» с поддержкой до 5 млн ₽ на ранней стадии, включая отдельные направления ИИ и цифровых технологий.

Для ClassOS это достаточно релевантно.

---

#### №3 Сколково

У Сколково в 2026 году существуют программы минигрантов, включая показательные внедрения до 10 млн ₽ при выполнении соответствующих условий.

Есть также механизм компенсации части инвестиций бизнес-ангелам в подходящие стартапы-резиденты.

Это может значительно упростить angel round.

---

## 55. Когда брать angel money

Не:

> есть Figma.

Не:

> написали агент.

А примерно когда есть:

```text
5–10 schools

100–300 devices

>1 000 managed classroom hours

paying customers

retention signal
```

Тогда капитал уже покупает **growth**, а не надежду.

---

## 56. На что тратить первый раунд

Не на маркетинг.

Основные расходы:

### Windows engineering

Очень сильный native Windows/Rust/C++ инженер.

#### Security

Потому что LocalSystem remote-management agent — потенциально опасное ПО.

#### Product/design

Teacher Console должен быть чрезвычайно простым.

#### Deployment

Installation/update mechanism.

#### Founder-led sales

Пока без sales department.

---

## 57. Кого я бы нанял первым

Учитывая, что web/cloud часть технический founder может закрыть самостоятельно, первый ключевой найм:

## **Senior Windows Systems Engineer.**

Знание:

* Win32;
* Windows Services;
* sessions;
* DXGI;
* Windows security;
* AppLocker/App Control;
* networking;
* installers;
* code signing.

Он намного ценнее первого ML engineer.

---

## 58. Кого НЕ нанимать сначала

Не нужны:

* ML team;
* sales team;
* marketing department;
* HR;
* mobile developers;
* data scientists.

До PMF это burn.

---

## 59. Seed

Seed имеет смысл после:

```text
20–50 organizations
+
thousands of devices
+
repeatable sales
+
very low churn
```

Тогда деньги идут на:

* enterprise;
* security;
* Linux;
* integrations;
* reseller network;
* international expansion.

---

## 60. Strategic investment

Особенно интересный вариант:

* крупная EdTech сеть;
* производитель ПК;
* образовательный интегратор;
* разработчик образовательной ОС.

Но:

## не отдавать эксклюзив

Например KIBERone может предложить:

> инвестируем, но только для нас.

Это фактически убивает TAM.

Можно дать:

* discount;
* early features;
* advisory status;
* branded integration.

Но не рынок целиком.

---

## 61. Exit opportunities

Категория уже имеет M&A историю.

Impero приобрёл Netop для расширения classroom/network-management portfolio.

Lenovo ранее приобрёл Stoneware, которому принадлежал LanSchool.

GoGuardian также рос через серию приобретений образовательных продуктов.

Потенциальные классы покупателей ClassOS когда-нибудь:

```text
EdTech platforms
MDM vendors
security vendors
device manufacturers
education hardware vendors
LMS companies
large education networks
```

То есть M&A path существует.

---

## 62. Главные риски

### Risk #1

#### Veyon бесплатный

Ответ:

не продавать remote-control.

Продавать lesson automation.

---

### Risk #2

#### NetSupport/Senso уже умеют многое

Ответ:

technical education specialization + lesson context.

---

### Risk #3

#### Microsoft может это сделать

Да.

Intune for Education уже умеет управлять школьными устройствами и приложениями, enrollment и политиками.

Поэтому ClassOS нельзя строить как generic MDM.

Нужно быть уровнем **над Intune/Windows**, который понимает занятие.

В будущем ClassOS даже может использовать Intune как enforcement provider.

---

## 63. Risk #4

### Windows engineering сложный

Это реальный риск.

Нужно поддерживать:

* обновления;
* multi-monitor;
* DPI;
* GPU;
* sessions;
* UAC;
* antivirus;
* firewall;
* sleep;
* network failures.

Поэтому нельзя недооценивать Agent.

---

## 64. Risk #5

### Security

ClassOS agent потенциально умеет:

* видеть экран;
* управлять компьютером;
* запускать процессы;
* менять policies.

Следовательно, compromised ClassOS = disaster.

Нужны:

```text
signed binaries
mTLS
RBAC
short-lived tokens
audit
device certificates
signed commands where appropriate
secure update channel
```

Security — product feature.

---

## 65. Risk #6

### Surveillance reputation

Очень опасно превратить продукт в:

> spyware для детей.

Я бы принципиально запретил:

* скрытый monitoring;
* keylogging;
* постоянную запись экранов;
* микрофон без явной необходимости;
* камеру;
* monitoring после занятия.

Student должен видеть:

> Teacher connected.

Это и этически правильнее, и сильно облегчает продажи.

---

## 66. Risk #7

### AI hype

Если первый pitch:

> AI classroom...

покупатель может не понять value.

Поэтому:

```text
Product first
AI second.
```

---

## 67. Самая важная продуктовая метрика

### Managed Classroom Hours

Например:

```text
Week 34

1 482 hours
```

Это означает:

> реальные занятия действительно зависят от ClassOS.

Именно эта метрика показывает embeddedness.

---

## 68. Вторичные метрики

```text
Active devices
Active classrooms
Lessons managed
Teacher WAU
Commands per lesson
Focus activations
Repair actions
AI interventions
```

---

## 69. PMF metrics

Особенно:

### 30-day school retention

и:

### weekly teacher retention

Если организация заплатила, но преподаватели не открывают ClassOS — продукта нет.

---

## 70. Главный enterprise KPI

### Device penetration within customer

Например школа:

```text
60 PCs

ClassOS:
12
```

не идеально.

Хочется:

```text
60/60
```

Когда система становится стандартом инфраструктуры, churn резко усложняется.

---

## 71. Реалистичный план первых 12 месяцев

### Месяц 1–2

Technical prototype.

```text
screen
remote
commands
agent
```

---

### Месяц 3

Первый реальный classroom pilot.

---

### Месяц 4

Focus + policies.

Цель:

> удалить Veyon.

---

### Месяц 5

Device health + software profiles.

---

### Месяц 6

5 филиалов.

Начинается платная модель.

---

### Месяц 7–8

Lesson Engine.

---

### Месяц 9

AlfaCRM.

---

### Месяц 10

Rule-based stuck detection.

---

### Месяц 11

AI Supervisor.

---

### Месяц 12

20+ школ — либо чёткое понимание, почему продажи не идут.

---

## 72. Условный target к концу первого года

Я бы поставил:

```text
20 organizations

500–1 000 devices

>20 000 managed classroom hours

>75% teacher WAU

<2% monthly logo churn
```

и где-то:

```text
300–600k ₽ MRR
```

в зависимости от pricing.

Это уже интересная seed-stage компания.

---

## 73. Три возможных исхода

### Сценарий A — маленький

Получается просто хороший Veyon replacement.

```text
5–20k устройств
```

Это всё равно может быть прибыльный SaaS.

---

### Сценарий B — хороший

ClassOS становится стандартом для частных технических школ.

```text
100k+ устройств
```

При 500 ₽: **50 млн ₽ MRR**.

Это уже очень серьёзный бизнес.

---

### Сценарий C — большой

ClassOS превращается в global learning-device platform.

```text
millions of endpoints
```

И конкурирует уже с:

* GoGuardian;
* Senso;
* LanSchool;
* NetSupport;
* Impero.

Это venture-scale outcome.

---

## 74. Что я считаю главным инсайтом проекта

Мы начинали с:

> «давайте сделаем Veyon нормально».

Это недостаточно.

Потом:

> «давайте управлять Windows».

Уже лучше.

Но настоящий продукт появился здесь:

## **ClassOS управляет не компьютерами. ClassOS управляет занятием через компьютеры.**

Это очень важное различие.

---

## 75. Финальное позиционирование

Не:

> MDM.

Не:

> Remote Desktop.

Не:

> parental control.

Не:

> Veyon alternative.

Не:

> LMS.

Не:

> AI tutor.

А:

## **ClassOS**

### Operating system for computer classrooms

Под ним остаётся Windows.

Над ним остаётся AlfaCRM/LMS.

А ClassOS находится ровно посередине:

```text
        School business systems
        AlfaCRM / LMS / Schedule
                 │
                 ▼

             CLASSOS

        Lesson Intelligence
        Classroom Control
        AI Supervisor
        Device Management

                 │
                 ▼

              Windows

                 │
                 ▼

         Student computers
```

---

## 76. Решение: делать или нет

### Я бы делал

Причины:

1. Проблему ты видел собственными глазами внутри реальной IT-школы.
2. Первый пользователь абсолютно понятен.
3. Можно получить design partner без массового маркетинга.
4. MVP технически сложный, но вполне реализуемый.
5. Даже базовый продукт имеет самостоятельную ценность.
6. AI не является фундаментальной зависимостью.
7. Есть естественный путь expansion.
8. Есть уже подтверждённая международная software category.
9. Есть примеры миллиардной оценки компании в соседнем сегменте.
10. Есть потенциальный российский, государственный и международный рынки.

Но я бы дал проекту **жёсткий checkpoint**:

> Сделать минимальный Veyon replacement + Focus/Lesson Mode и поставить его в одном реальном компьютерном классе.

Не строить полгода платформу.

Не искать инвестиции.

Не писать AI.

Не интегрировать AlfaCRM.

Сначала получить момент:

> преподаватель поработал две недели и **не хочет возвращаться в Veyon**.

Если это произошло — я бы уже серьёзно вкладывался в ClassOS и начинал строить компанию вокруг него.

Если нет — мы дешево узнали, что hypothesis ошибочна.

Это сейчас самый правильный тест всей идеи.
