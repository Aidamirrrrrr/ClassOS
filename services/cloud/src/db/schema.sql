-- Схема Cloud v0 (spec T8 §4.1).
--
-- Ключевое ограничение: PostgreSQL НИКОГДА не хранит приватный ключ
-- устройства (§4.2, инвариант T8 §12.1-12.2). Здесь есть только публичный
-- сертификат и его отпечаток. Колонки для приватного ключа нет намеренно —
-- её отсутствие является частью контракта, а не упущением.

-- citext даёт регистронезависимую уникальность email без отдельного индекса
-- по lower(email). Расширение обязано существовать до создания таблиц.
CREATE EXTENSION IF NOT EXISTS citext;

CREATE TABLE organizations (
    id              uuid PRIMARY KEY,
    name            text NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE users (
    id              uuid PRIMARY KEY,
    email           citext NOT NULL UNIQUE,
    password_hash   text NOT NULL,
    display_name    text NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);

-- Сессии хранятся хешем токена: дамп таблицы не должен давать вход в систему
-- (spec T8 §12.1). Сам токен существует только у клиента.
CREATE TABLE sessions (
    token_hash      bytea PRIMARY KEY,
    user_id         uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    expires_at      timestamptz NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX sessions_user ON sessions (user_id);

CREATE TABLE branches (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            text NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);

-- Членство со скоупом: branch_id IS NULL означает права на всю организацию.
CREATE TABLE organization_users (
    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    user_id         uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    branch_id       uuid REFERENCES branches(id) ON DELETE CASCADE,
    role            text NOT NULL CHECK (role IN ('owner', 'admin', 'teacher')),
    PRIMARY KEY (organization_id, user_id, COALESCE(branch_id, '00000000-0000-0000-0000-000000000000'::uuid))
);

CREATE TABLE rooms (
    id              uuid PRIMARY KEY,
    branch_id       uuid NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
    name            text NOT NULL,
    update_channel  text NOT NULL DEFAULT 'stable'
        CHECK (update_channel IN ('stable', 'beta', 'canary')),
    desired_profile_id text,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE devices (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    branch_id       uuid NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
    room_id         uuid REFERENCES rooms(id) ON DELETE SET NULL,
    hostname        text NOT NULL,
    agent_version   text,
    last_seen_at    timestamptz,
    health_state    text CHECK (health_state IN ('healthy', 'warning', 'critical')),
    created_at      timestamptz NOT NULL DEFAULT now()
);

-- Только публичная часть identity устройства.
CREATE TABLE device_certificates (
    device_id       uuid PRIMARY KEY REFERENCES devices(id) ON DELETE CASCADE,
    certificate_der bytea NOT NULL,
    fingerprint_sha256 bytea NOT NULL UNIQUE,
    issued_at       timestamptz NOT NULL DEFAULT now(),
    expires_at      timestamptz NOT NULL
);

CREATE TABLE policies (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            text NOT NULL,
    document        jsonb NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE room_policies (
    room_id         uuid NOT NULL REFERENCES rooms(id) ON DELETE CASCADE,
    policy_id       uuid NOT NULL REFERENCES policies(id) ON DELETE CASCADE,
    PRIMARY KEY (room_id, policy_id)
);

-- Placeholder: наполнится в фазе Lesson Engine.
CREATE TABLE lesson_profiles (
    id              uuid PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    name            text NOT NULL,
    definition      jsonb NOT NULL DEFAULT '{}'::jsonb
);

CREATE TABLE enrollment_tokens (
    code_hash       bytea PRIMARY KEY,
    organization_id uuid NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    branch_id       uuid NOT NULL REFERENCES branches(id) ON DELETE CASCADE,
    room_id         uuid REFERENCES rooms(id) ON DELETE SET NULL,
    created_by      uuid NOT NULL REFERENCES users(id),
    expires_at      timestamptz NOT NULL,
    -- Одноразовость обеспечивается этим полем, а не удалением строки:
    -- использованный код должен оставаться видимым в аудите.
    used_at         timestamptz,
    used_by_device  uuid REFERENCES devices(id) ON DELETE SET NULL
);

CREATE TABLE agent_versions (
    version         text NOT NULL,
    release_channel text NOT NULL CHECK (release_channel IN ('stable', 'beta', 'canary')),
    url             text NOT NULL,
    sha256          text NOT NULL,
    signature       bytea NOT NULL,
    minimum_supported_version text NOT NULL,
    published_at    timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (version, release_channel)
);

CREATE TABLE audit_events (
    id              bigserial PRIMARY KEY,
    organization_id uuid REFERENCES organizations(id) ON DELETE CASCADE,
    actor_user_id   uuid REFERENCES users(id) ON DELETE SET NULL,
    device_id       uuid REFERENCES devices(id) ON DELETE SET NULL,
    action          text NOT NULL,
    outcome         text NOT NULL CHECK (outcome IN ('success', 'failure')),
    details         jsonb NOT NULL DEFAULT '{}'::jsonb,
    occurred_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_org_time ON audit_events (organization_id, occurred_at DESC);
CREATE INDEX devices_branch ON devices (branch_id);
CREATE INDEX rooms_branch ON rooms (branch_id);
