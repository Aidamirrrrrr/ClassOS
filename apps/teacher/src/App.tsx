import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Device = { device_id: string; hostname: string; ip: string; control_port: number; room_hint: string; agent_version: string };
type Code = { code: string; expires_at_unix_ms: number };
type FrameReady = { device_id: string; sequence: number };
type CommandResult = { device_id: string; command_id: string; success: boolean; error_code: string; message: string };

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

  const device = devices.find((value) => value.device_id === selectedDeviceId) ?? null;

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

  async function discover() {
    setMessage("Ищем устройства в локальной сети…");
    try {
      const found = await invoke<Device>("discover_device");
      setDevices((current) => current.some((value) => value.device_id === found.device_id) ? current : [...current, found]);
      setSelectedDeviceId(found.device_id);
      setMessage("Устройство найдено");
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
      setMessage(failed.length === 0 ? `Команда выполнена: ${successful}/${results.length}` : `Команда: ${successful}/${results.length}; ${failed.map((result) => `${result.device_id}: ${result.error_code}`).join(", ")}`);
    } catch (error) { setMessage(String(error)); }
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
    <p className="subtitle">Обнаружение, enrollment и live screen stream</p>
    <section className="panel">
      <button onClick={discover}>Найти устройство</button>
      <button onClick={createCode}>Создать enrollment-код</button>
      <button onClick={startGrid} disabled={devices.length === 0}>Запустить grid</button>
      <button onClick={selectAllCommandDevices} disabled={devices.length === 0}>{commandDeviceIds.length === devices.length ? "Снять выбор" : "Выбрать всех"}</button>
      <button onClick={() => void runCommand("lock", "", commandDeviceIds)} disabled={commandDeviceIds.length === 0}>Заблокировать выбранных</button>
      <button onClick={() => void runCommand("unlock", "", commandDeviceIds)} disabled={commandDeviceIds.length === 0}>Разблокировать выбранных</button>
      {code && <p className="code">Код: <strong>{code.code}</strong></p>}
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
      <button onClick={() => void runCommand("restart")}>Перезагрузить</button>
      <button onClick={() => void runCommand("shutdown")}>Выключить</button>
      <button onClick={takeControl} disabled={controllingDeviceId === device.device_id}>Взять управление</button>
      <button onClick={stopControl} disabled={controllingDeviceId !== device.device_id}>Остановить управление</button>
      {frameUrls[device.device_id] && <img className="screenshot" src={frameUrls[device.device_id]} onPointerMove={movePointer} onPointerDown={clickPointer} onWheel={wheelPointer} alt="Снимок экрана устройства" />}
    </section>}
    <p className="status">{message}</p>
  </main>;
}

export default App;
