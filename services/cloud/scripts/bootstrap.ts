#!/usr/bin/env bun
/**
 * Первичное развёртывание Cloud v0: схема БД и первый владелец организации.
 *
 * Без этого шага развёрнутый Cloud пуст и войти в него нечем: регистрации
 * пользователей в API нет намеренно — школьный Cloud не открытый сервис, и
 * маршрут, создающий владельца организации без аутентификации, был бы дырой,
 * а не удобством (spec T8 §5). Поэтому первый владелец заводится оператором
 * при развёртывании, а всех остальных заводит уже он.
 *
 * Скрипт идемпотентен: повторный запуск не пересоздаёт схему и не меняет
 * пароль существующего пользователя, а сообщает, что всё уже на месте.
 *
 * Использование:
 *   DATABASE_URL=postgres://... bun scripts/bootstrap.ts \
 *     --organization "IT-школа" \
 *     --email owner@example.org \
 *     --password '<пароль>'
 *
 * Пароль можно не передавать аргументом — тогда он читается из
 * CLASSOS_BOOTSTRAP_PASSWORD, что не оставляет его в истории командной
 * оболочки и в списке процессов.
 */

import { randomUUID } from "node:crypto";

import { PostgresStore } from "../src/db/postgres";
import { hashPassword } from "../src/domain/auth";

function argument(name: string): string | undefined {
  const index = process.argv.indexOf(`--${name}`);
  return index === -1 ? undefined : process.argv[index + 1];
}

function fail(message: string): never {
  console.error(message);
  process.exit(2);
}

const databaseUrl = process.env.DATABASE_URL;
if (!databaseUrl) {
  fail(
    "DATABASE_URL не задан. Bootstrap работает только против настоящей базы: " +
      "in-memory хранилище не переживёт перезапуск, и заведённый в нём владелец исчез бы вместе с процессом.",
  );
}

const organizationName = argument("organization");
if (!organizationName) fail("не задан обязательный аргумент --organization");

const email = argument("email");
if (!email) fail("не задан обязательный аргумент --email");

const password = argument("password") ?? process.env.CLASSOS_BOOTSTRAP_PASSWORD;
if (!password) {
  fail("не задан пароль: передайте --password или CLASSOS_BOOTSTRAP_PASSWORD");
}
if (password.length < 12) {
  fail("пароль владельца короче 12 символов — это учётная запись с полными правами на организацию");
}

const displayName = argument("display-name") ?? email;

const store = new PostgresStore(databaseUrl);

try {
  // Схема применяется отсюда, а не при старте сервиса: миграция — операция
  // развёртывания, и выполнять её на каждом запуске процесса означало бы
  // менять базу в момент, когда этого никто не ожидает.
  if (await store.isInitialized()) {
    console.log("Схема уже применена — пропускаем миграцию.");
  } else {
    const schema = await Bun.file(new URL("../src/db/schema.sql", import.meta.url)).text();
    await store.migrate(schema);
    console.log("Схема применена.");
  }

  const existing = await store.findUserByEmail(email);
  if (existing) {
    console.log(`Пользователь ${email} уже существует — пароль и права не изменены.`);
    console.log("Готово.");
    process.exit(0);
  }

  const organizationId = randomUUID();
  const userId = randomUUID();

  await store.createOrganization({ id: organizationId, name: organizationName });
  await store.createUser({
    id: userId,
    email,
    passwordHash: await hashPassword(password),
    displayName,
  });
  // branchId: null — членство действует на всю организацию, иначе владелец не
  // смог бы создать первый филиал (проверка прав требует скоуп, а филиалов
  // ещё нет).
  await store.addMembership({ organizationId, userId, branchId: null, role: "owner" });

  console.log(`Организация «${organizationName}» создана: ${organizationId}`);
  console.log(`Владелец ${email} создан: ${userId}`);
  console.log("");
  console.log("Дальше: войти в Teacher Console этой почтой и паролем, создать филиал и кабинет.");
} finally {
  await store.close();
}
