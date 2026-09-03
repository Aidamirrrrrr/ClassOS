import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import "./App.css";

type Device = { device_id: string; hostname: string; ip: string; control_port: number; room_hint: string; agent_version: string };
type Code = { code: string; expires_at_unix_ms: number };

function App() {
  const [device, setDevice] = useState<Device | null>(null);
  const [code, setCode] = useState<Code | null>(null);
  const [message, setMessage] = useState("Готово к работе");
  const [enrollmentCode, setEnrollmentCode] = useState("");
  const [screenshotUrl, setScreenshotUrl] = useState<string | null>(null);

  async function discover() {
    setMessage("Ищем устройства в локальной сети…");
    try { setDevice(await invoke<Device>("discover_device")); setMessage("Устройство найдено"); }
    catch (error) { setMessage(String(error)); }
  }

  async function createCode() {
    try { const value = await invoke<Code>("create_enrollment_code"); setCode(value); setEnrollmentCode(value.code); setMessage("Код создан; введите его на Student PC"); }
    catch (error) { setMessage(String(error)); }
  }

  async function enroll() {
    if (!device) return;
    setMessage("Выпускаем credential…");
    try { setMessage(await invoke<string>("enroll_device", { deviceId: device.device_id, ip: device.ip, controlPort: device.control_port, code: enrollmentCode })); }
    catch (error) { setMessage(String(error)); }
  }

  async function screenshot() {
    if (!device) return;
    setMessage("Запрашиваем снимок экрана…");
    try {
      const bytes = await invoke<number[]>("request_screenshot", { deviceId: device.device_id, displayId: 0 });
      const blob = new Blob([new Uint8Array(bytes)], { type: "image/jpeg" });
      setScreenshotUrl(URL.createObjectURL(blob));
      setMessage("Снимок получен");
    } catch (error) { setMessage(String(error)); }
  }

  return <main className="container">
    <h1>ClassOS Teacher Console</h1>
    <p className="subtitle">Сетевое обнаружение и enrollment устройств (T1)</p>
    <section className="panel">
      <button onClick={discover}>Найти устройство</button>
      <button onClick={createCode}>Создать enrollment-код</button>
      {code && <p className="code">Код: <strong>{code.code}</strong></p>}
    </section>
    {device && <section className="panel">
      <h2>{device.hostname}</h2>
      <p>{device.ip}:{device.control_port} · {device.device_id}</p>
      <label>Enrollment-код<input value={enrollmentCode} onChange={(event) => setEnrollmentCode(event.currentTarget.value)} /></label>
      <button onClick={enroll} disabled={!enrollmentCode}>Зарегистрировать устройство</button>
      <button onClick={screenshot}>Сделать снимок экрана</button>
      {screenshotUrl && <img className="screenshot" src={screenshotUrl} alt="Снимок экрана устройства" />}
    </section>}
    <p className="status">{message}</p>
  </main>;
}

export default App;
