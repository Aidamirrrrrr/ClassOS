import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Device = { device_id: string; hostname: string; ip: string; control_port: number; room_hint: string; agent_version: string };
type Code = { code: string; expires_at_unix_ms: number };
type FrameReady = { device_id: string; sequence: number };

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

  return <main className="container">
    <h1>ClassOS Teacher Console</h1>
    <p className="subtitle">Обнаружение, enrollment и live screen stream</p>
    <section className="panel">
      <button onClick={discover}>Найти устройство</button>
      <button onClick={createCode}>Создать enrollment-код</button>
      <button onClick={startGrid} disabled={devices.length === 0}>Запустить grid</button>
      {code && <p className="code">Код: <strong>{code.code}</strong></p>}
    </section>
    {devices.length > 0 && <section className="grid" aria-label="Экраны устройств">
      {devices.map((value) => <button className={`device-card ${value.device_id === selectedDeviceId ? "selected" : ""}`} key={value.device_id} onClick={() => setSelectedDeviceId(value.device_id)}>
        <strong>{value.hostname}</strong>
        <small>{value.ip}:{value.control_port}</small>
        {frameUrls[value.device_id] ? <img className="thumbnail" src={frameUrls[value.device_id]} alt={`Экран ${value.hostname}`} /> : <span>Нет кадра</span>}
      </button>)}
    </section>}
    {device && <section className="panel">
      <h2>{device.hostname}</h2>
      <p>{device.ip}:{device.control_port} · {device.device_id}</p>
      <label>Enrollment-код<input value={enrollmentCode} onChange={(event) => setEnrollmentCode(event.currentTarget.value)} /></label>
      <button onClick={enroll} disabled={!enrollmentCode}>Зарегистрировать устройство</button>
      <button onClick={screenshot}>Сделать снимок экрана</button>
      <button onClick={() => startStream(device.device_id, true)}>Открыть live view</button>
      <button onClick={() => stopStream(device.device_id)}>Остановить поток</button>
      {frameUrls[device.device_id] && <img className="screenshot" src={frameUrls[device.device_id]} alt="Снимок экрана устройства" />}
    </section>}
    <p className="status">{message}</p>
  </main>;
}

export default App;
