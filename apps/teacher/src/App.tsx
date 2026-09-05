import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Device = { device_id: string; hostname: string; ip: string; control_port: number; room_hint: string; agent_version: string };
type Code = { code: string; expires_at_unix_ms: number };
type Membership = { organizationId: string; branchId: string | null; role: string };
type FrameReady = { device_id: string; sequence: number };
type RepairItem = { application_id: string; success: boolean; error_code: string };
type CommandResult = { device_id: string; command_id: string; success: boolean; error_code: string; message: string; repair: RepairItem[] };
type Drift = { application_id: string; kind: string; required_version: string; actual_version: string };
type Health = {
  device_id: string; state: string; cpu_percent: number; ram_percent: number; disk_percent: number;
  os_version: string; agent_version: string; uptime_seconds: number; profile_id: string;
  warnings: string[]; drift: Drift[];
};

// Устройство сообщает машиночитаемые коды; человекочитаемый текст —
// ответственность UI, а не агента (spec T7 §10.3).
const WARNING_TEXT: Record<string, string> = {
  DISK_SPACE_LOW: "Мало места на диске",
  DISK_SPACE_CRITICAL: "Диск почти заполнен",
  MEMORY_PRESSURE: "Не хватает оперативной памяти",
  CPU_SATURATED: "Процессор перегружен",
  SOFTWARE_MISSING: "Не хватает программ по профилю",
  SOFTWARE_VERSION_MISMATCH: "Версия программы не соответствует профилю",
  SOFTWARE_MANAGER_UNAVAILABLE: "Не удалось проверить установленные программы",
  POLICY_APPLY_FAILED: "Не удалось применить политику урока",
  NO_INTERACTIVE_SESSION: "Никто не вошёл в систему",
};

const STATE_TEXT: Record<string, string> = {
  healthy: "Исправен",
  warning: "Требует внимания",
  critical: "Проблема",
  unknown: "Неизвестно",
};

function versionedUrl(url: string, sequence: number) {
  return `${url}?sequence=${sequence}`;
}

function App() {
  const [devices, setDevices] = useState<Device[]>([]);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [code, setCode] = useState<Code | null>(null);
  const [message, setMessage] = useState("Готово к работе");
  const [enrollmentCode, setEnrollmentCode] = useState("");
  const [frameUrls, setFrameUrls] = useState<Record<string, string>>({});
  const [controllingDeviceId, setControllingDeviceId] = useState<string | null>(null);
  const [commandDeviceIds, setCommandDeviceIds] = useState<string[]>([]);
  const [health, setHealth] = useState<Record<string, Health>>({});
  const [discovering, setDiscovering] = useState(false);
  const [cloudUrl, setCloudUrl] = useState("http://localhost:8787");
  const [cloudEmail, setCloudEmail] = useState("");
  const [cloudPassword, setCloudPassword] = useState("");
  const [memberships, setMemberships] = useState<Membership[]>([]);
  const [leaseExpiresAt, setLeaseExpiresAt] = useState<number | null>(null);

  const device = devices.find((value) => value.device_id === selectedDeviceId) ?? null;
  // Политика урока по умолчанию применяется ко всему классу: выбор отдельных
  // устройств — уточнение, а не обязательный шаг (T6 DoD §2).
  const policyTargets = commandDeviceIds.length > 0 ? commandDeviceIds : devices.map((value) => value.device_id);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<FrameReady>("stream-frame-ready", (event) => {
      setFrameUrls((current) => {
        const url = current[event.payload.device_id];
        return url
          ? { ...current, [event.payload.device_id]: versionedUrl(url.split("?")[0], event.payload.sequence) }
          : current;
      });
    }).then((dispose) => { unlisten = dispose; });
    return () => { unlisten?.(); };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<string>("stream-status", (event) => setMessage(event.payload)).then((dispose) => { unlisten = dispose; });
    return () => { unlisten?.(); };
  }, []);

  // Устройства объявляют себя независимо, поэтому класс собирается
  // постепенно. Повторное объявление обновляет адрес: устройство могло
  // получить новый IP после перезагрузки.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<Device>("device-discovered", (event) => {
      setDevices((current) => {
        const index = current.findIndex((value) => value.device_id === event.payload.device_id);
        if (index === -1) return [...current, event.payload];
        const updated = [...current];
        updated[index] = event.payload;
        return updated;
      });
    }).then((dispose) => { unlisten = dispose; });
    return () => { unlisten?.(); };
  }, []);

  useEffect(() => {
    let unlisten: (() => void) | undefined;
    void listen<string>("discovery-status", (event) => {
      setDiscovering(false);
      setMessage(event.payload);
    }).then((dispose) => { unlisten = dispose; });
    return () => { unlisten?.(); };
  }, []);

  useEffect(() => {
    if (!controllingDeviceId) return;
    const sendKey = (event: KeyboardEvent, isDown: boolean) => {
      if (isDown && event.repeat) return;
      event.preventDefault();
      void invoke("send_remote_key", { deviceId: controllingDeviceId, virtualKeyCode: event.keyCode, isDown });
    };
    const keyDown = (event: KeyboardEvent) => sendKey(event, true);
    const keyUp = (event: KeyboardEvent) => sendKey(event, false);
    window.addEventListener("keydown", keyDown, { capture: true });
    window.addEventListener("keyup", keyUp, { capture: true });
    return () => {
      window.removeEventListener("keydown", keyDown, { capture: true });
      window.removeEventListener("keyup", keyUp, { capture: true });
    };
  }, [controllingDeviceId]);

  async function toggleDiscovery() {
    try {
      if (discovering) {
        await invoke("stop_discovery");
        setDiscovering(false);
        setMessage(`Поиск остановлен; устройств в списке: ${devices.length}`);
        return;
      }
      await invoke("start_discovery");
      setDiscovering(true);
      setMessage("Ищем устройства в локальной сети… список пополняется сам");
    } catch (error) { setMessage(String(error)); }
  }

  async function signIn() {
    setMessage("Входим в Cloud…");
    try {
      const found = await invoke<Membership[]>("cloud_sign_in", { baseUrl: cloudUrl, email: cloudEmail, password: cloudPassword });
      setMemberships(found);
      setCloudPassword("");
      setMessage(found.length > 0 ? `Вход выполнен; филиалов доступно: ${found.length}` : "Вход выполнен, но доступных филиалов нет");
    } catch (error) { setMessage(String(error)); }
  }

  async function signOut() {
    await invoke("cloud_sign_out");
    setMemberships([]);
    setLeaseExpiresAt(null);
    setMessage("Выход из Cloud выполнен");
  }

  // Lease нужен, чтобы вести урок на устройствах, зарегистрированных через
  // Cloud: без него они откажут в подключении.
  async function issueLease(membership: Membership) {
    if (!membership.branchId) {
      setMessage("Для этой роли нужен конкретный филиал");
      return;
    }
    try {
      const expiresAt = await invoke<number>("cloud_issue_lease", { organizationId: membership.organizationId, branchId: membership.branchId });
      setLeaseExpiresAt(expiresAt);
      setMessage(`Доступ к кабинету получен до ${new Date(expiresAt).toLocaleString()}`);
    } catch (error) { setMessage(String(error)); }
  }

  async function createCloudCode(membership: Membership) {
    if (!membership.branchId) {
      setMessage("Для этой роли нужен конкретный филиал");
      return;
    }
    try {
      const value = await invoke<Code>("cloud_create_enrollment_code", { branchId: membership.branchId, roomId: null });
      setCode(value);
      setEnrollmentCode(value.code);
      setMessage("Код создан в Cloud; введите его на Student PC");
    } catch (error) { setMessage(String(error)); }
  }

  async function createCode() {
    try {
      const value = await invoke<Code>("create_enrollment_code");
      setCode(value);
      setEnrollmentCode(value.code);
      setMessage("Код создан; введите его на Student PC");
    } catch (error) { setMessage(String(error)); }
  }

  async function enroll() {
    if (!device) return;
    setMessage("Выпускаем credential…");
    try {
      setMessage(await invoke<string>("enroll_device", { deviceId: device.device_id, ip: device.ip, controlPort: device.control_port, code: enrollmentCode }));
    } catch (error) { setMessage(String(error)); }
  }

  async function screenshot() {
    if (!device) return;
    setMessage("Запрашиваем снимок экрана…");
    try {
      const url = await invoke<string>("request_screenshot", { deviceId: device.device_id, displayId: 0 });
      setFrameUrls((current) => ({ ...current, [device.device_id]: versionedUrl(url, Date.now()) }));
      setMessage("Снимок получен");
    } catch (error) { setMessage(String(error)); }
  }

  async function startStream(deviceId: string, selected: boolean) {
    try {
      const url = await invoke<string>("start_stream", { deviceId, selected });
      setFrameUrls((current) => ({ ...current, [deviceId]: versionedUrl(url, Date.now()) }));
      setMessage(selected ? "Открыт live view" : "Запущен thumbnail-поток");
    } catch (error) {
      setMessage(String(error));
      throw error;
    }
  }

  async function startGrid() {
    const results = await Promise.allSettled(devices.map((value) => startStream(value.device_id, false)));
    const failed = results.filter((result) => result.status === "rejected").length;
    setMessage(failed === 0 ? `${devices.length} / ${devices.length} устройств в grid` : `${devices.length - failed} / ${devices.length} устройств в grid; часть потоков не запустилась`);
  }

  async function stopStream(deviceId: string) {
    await invoke("stop_stream", { deviceId });
    setMessage("Поток остановлен");
  }

  async function runCommand(kind: string, value = "", deviceIds = device ? [device.device_id] : []) {
    if (deviceIds.length === 0) return;
    try {
      const results = await invoke<CommandResult[]>("dispatch_classroom_command", { deviceIds, kind, value });
      const successful = results.filter((result) => result.success).length;
      const failed = results.filter((result) => !result.success);
      // Repair может завершиться успешно как команда и при этом не поставить
      // часть приложений: показывать только «выполнено» здесь нельзя.
      const brokenPackages = results.flatMap((result) =>
        (result.repair ?? []).filter((item) => !item.success).map((item) => `${result.device_id}: ${item.application_id} (${item.error_code})`),
      );
      const summary = failed.length === 0
        ? `Команда выполнена: ${successful}/${results.length}`
        : `Команда: ${successful}/${results.length}; ${failed.map((result) => `${result.device_id}: ${result.error_code}`).join(", ")}`;
      setMessage(brokenPackages.length === 0 ? summary : `${summary}; не установлено — ${brokenPackages.join(", ")}`);
    } catch (error) { setMessage(String(error)); }
  }

  async function checkHealth(deviceIds: string[]) {
    setMessage("Опрашиваем состояние устройств…");
    const results = await Promise.allSettled(deviceIds.map(async (deviceId) => {
      const report = await invoke<Health>("request_health", { deviceId });
      setHealth((current) => ({ ...current, [deviceId]: report }));
    }));
    const failed = results.filter((result) => result.status === "rejected").length;
    setMessage(failed === 0
      ? `Состояние получено: ${deviceIds.length}`
      : `Состояние получено: ${deviceIds.length - failed}/${deviceIds.length}`);
  }

  // Перезагрузка и выключение необратимы для того, кто в этот момент работает
  // за компьютером: один промах мышью не должен их запускать.
  function confirmAnd(question: string, kind: string) {
    if (!device) return;
    if (window.confirm(`${question}\n\n${device.hostname}`)) void runCommand(kind);
  }

  function toggleCommandDevice(deviceId: string) {
    setCommandDeviceIds((current) => current.includes(deviceId)
      ? current.filter((value) => value !== deviceId)
      : [...current, deviceId]);
  }

  function selectAllCommandDevices() {
    setCommandDeviceIds((current) => current.length === devices.length ? [] : devices.map((value) => value.device_id));
  }

  async function takeControl() {
    if (!device) return;
    try { await invoke("start_remote_control", { deviceId: device.device_id }); setControllingDeviceId(device.device_id); setMessage("Remote control активен"); }
    catch (error) { setMessage(String(error)); }
  }

  async function stopControl() {
    if (!device) return;
    await invoke("stop_remote_control", { deviceId: device.device_id });
    setControllingDeviceId(null);
    setMessage("Remote control остановлен");
  }

  function movePointer(event: React.PointerEvent<HTMLImageElement>) {
    if (!device || controllingDeviceId !== device.device_id) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    const y = Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height));
    void invoke("send_remote_mouse_move", { deviceId: device.device_id, x, y });
  }

  function clickPointer(event: React.PointerEvent<HTMLImageElement>) {
    if (!device || controllingDeviceId !== device.device_id) return;
    const rect = event.currentTarget.getBoundingClientRect();
    const x = Math.min(1, Math.max(0, (event.clientX - rect.left) / rect.width));
    const y = Math.min(1, Math.max(0, (event.clientY - rect.top) / rect.height));
    const button = event.button === 2 ? 1 : event.button === 1 ? 2 : 0;
    void invoke("send_remote_mouse_button", { deviceId: device.device_id, button, isDown: true, x, y });
    void invoke("send_remote_mouse_button", { deviceId: device.device_id, button, isDown: false, x, y });
  }

  function wheelPointer(event: React.WheelEvent<HTMLImageElement>) {
    if (!device || controllingDeviceId !== device.device_id) return;
    event.preventDefault();
    void invoke("send_remote_wheel", { deviceId: device.device_id, delta: Math.round(-event.deltaY) });
  }

  return <main className="container">
    <h1>ClassOS Teacher Console</h1>
    <p className="subtitle">Экраны класса, режимы урока и состояние компьютеров</p>
    <section className="panel" aria-label="Cloud">
      <h2>Cloud</h2>
      {memberships.length === 0 ? <>
        <label>Адрес<input value={cloudUrl} onChange={(event) => setCloudUrl(event.currentTarget.value)} /></label>
        <label>Почта<input type="email" autoComplete="username" value={cloudEmail} onChange={(event) => setCloudEmail(event.currentTarget.value)} /></label>
        <label>Пароль<input type="password" autoComplete="current-password" value={cloudPassword} onChange={(event) => setCloudPassword(event.currentTarget.value)} /></label>
        <button onClick={signIn} disabled={!cloudEmail || !cloudPassword}>Войти</button>
        <p className="subtitle">Без Cloud консоль работает с устройствами, зарегистрированными локально.</p>
      </> : <>
        <p className="subtitle">{leaseExpiresAt
          ? `Доступ к кабинету действует до ${new Date(leaseExpiresAt).toLocaleTimeString()}`
          : "Доступ к кабинету не получен — устройства из Cloud откажут в подключении"}</p>
        {memberships.map((membership) => <div key={`${membership.organizationId}-${membership.branchId ?? "org"}`} className="membership">
          <span>{membership.role}{membership.branchId ? ` · филиал ${membership.branchId.slice(0, 8)}` : " · вся организация"}</span>
          <button onClick={() => void issueLease(membership)}>Получить доступ</button>
          <button onClick={() => void createCloudCode(membership)}>Код регистрации</button>
        </div>)}
        <button onClick={signOut}>Выйти</button>
      </>}
    </section>
    <section className="panel">
      <button onClick={toggleDiscovery}>{discovering ? "Остановить поиск" : "Найти устройства"}</button>
      <button onClick={createCode}>Создать локальный код</button>
      <button onClick={startGrid} disabled={devices.length === 0}>Запустить grid</button>
      <button onClick={selectAllCommandDevices} disabled={devices.length === 0}>{commandDeviceIds.length === devices.length ? "Снять выбор" : "Выбрать всех"}</button>
      <button onClick={() => void runCommand("lock", "", commandDeviceIds)} disabled={commandDeviceIds.length === 0}>Заблокировать выбранных</button>
      <button onClick={() => void runCommand("unlock", "", commandDeviceIds)} disabled={commandDeviceIds.length === 0}>Разблокировать выбранных</button>
      {code && <p className="code">Код: <strong>{code.code}</strong></p>}
    </section>
    <section className="panel" aria-label="Политика урока">
      <h2>Урок</h2>
      <p className="subtitle">{policyTargets.length > 0
        ? `Действует на устройств: ${policyTargets.length}`
        : "Нет устройств"}</p>
      <button onClick={() => void runCommand("policy", "python", policyTargets)} disabled={policyTargets.length === 0}>Python</button>
      <button onClick={() => void runCommand("policy", "web", policyTargets)} disabled={policyTargets.length === 0}>Web</button>
      <button onClick={() => void runCommand("focus", "vscode", policyTargets)} disabled={policyTargets.length === 0}>Focus Mode</button>
      <button onClick={() => void runCommand("focus_off", "", policyTargets)} disabled={policyTargets.length === 0}>Выключить Focus</button>
      <button onClick={() => void runCommand("policy_off", "", policyTargets)} disabled={policyTargets.length === 0}>Снять политику</button>
    </section>
    <section className="panel" aria-label="Состояние кабинета">
      <h2>Кабинет</h2>
      <button onClick={() => void checkHealth(policyTargets)} disabled={policyTargets.length === 0}>Проверить состояние</button>
      <button onClick={() => void runCommand("repair", "python-classroom", policyTargets)} disabled={policyTargets.length === 0}>Привести к профилю Python</button>
      {devices.length > 0 && <table className="health">
        <thead><tr><th>Устройство</th><th>Состояние</th><th>Диск</th><th>Что не так</th></tr></thead>
        <tbody>
          {devices.map((value) => {
            const report = health[value.device_id];
            return <tr key={value.device_id}>
              <td>{value.hostname}</td>
              <td>{report ? STATE_TEXT[report.state] ?? report.state : "—"}</td>
              <td>{report ? `${Math.round(report.disk_percent)}%` : "—"}</td>
              <td>{report
                ? [
                    ...report.warnings.map((code) => WARNING_TEXT[code] ?? code),
                    ...report.drift.map((entry) => entry.kind === "missing"
                      ? `нет ${entry.application_id}`
                      : `${entry.application_id}: ${entry.actual_version} вместо ${entry.required_version}`),
                  ].join("; ") || "—"
                : "—"}</td>
            </tr>;
          })}
        </tbody>
      </table>}
    </section>
    {devices.length > 0 && <section className="grid" aria-label="Экраны устройств">
      {devices.map((value) => <article className={`device-card ${value.device_id === selectedDeviceId ? "selected" : ""}`} key={value.device_id} onClick={() => setSelectedDeviceId(value.device_id)}>
        <label className="device-select" onClick={(event) => event.stopPropagation()}><input type="checkbox" checked={commandDeviceIds.includes(value.device_id)} onChange={() => toggleCommandDevice(value.device_id)} /> Выбрать для команд</label>
        <strong>{value.hostname}</strong>
        <small>{value.ip}:{value.control_port}</small>
        {frameUrls[value.device_id] ? <img className="thumbnail" src={frameUrls[value.device_id]} alt={`Экран ${value.hostname}`} /> : <span>Нет кадра</span>}
      </article>)}
    </section>}
    {device && <section className="panel">
      <h2>{device.hostname}</h2>
      <p>{device.ip}:{device.control_port} · {device.device_id}</p>
      <label>Enrollment-код<input value={enrollmentCode} onChange={(event) => setEnrollmentCode(event.currentTarget.value)} /></label>
      <button onClick={enroll} disabled={!enrollmentCode}>Зарегистрировать устройство</button>
      <button onClick={screenshot}>Сделать снимок экрана</button>
      <button onClick={() => startStream(device.device_id, true)}>Открыть live view</button>
      <button onClick={() => stopStream(device.device_id)}>Остановить поток</button>
      <button onClick={() => void runCommand("lock")}>Заблокировать</button>
      <button onClick={() => void runCommand("unlock")}>Разблокировать</button>
      <button onClick={() => { const text = window.prompt("Текст сообщения"); if (text) void runCommand("message", text); }}>Сообщение</button>
      <button onClick={() => void runCommand("application", "vscode")}>Открыть VS Code</button>
      <button onClick={() => { const url = window.prompt("HTTP(S) URL"); if (url) void runCommand("url", url); }}>Открыть URL</button>
      <button onClick={() => confirmAnd("Перезагрузить этот компьютер?", "restart")}>Перезагрузить</button>
      <button onClick={() => confirmAnd("Выключить этот компьютер?", "shutdown")}>Выключить</button>
      <button onClick={takeControl} disabled={controllingDeviceId === device.device_id}>Взять управление</button>
      <button onClick={stopControl} disabled={controllingDeviceId !== device.device_id}>Остановить управление</button>
      {frameUrls[device.device_id] && <img className="screenshot" src={frameUrls[device.device_id]} onPointerMove={movePointer} onPointerDown={clickPointer} onWheel={wheelPointer} alt="Снимок экрана устройства" />}
    </section>}
    <p className="status">{message}</p>
  </main>;
}

export default App;
