//! Диагностика пригодности машины для захвата экрана (T2–T4).
//!
//! Отвечает на единственный вопрос, который решает выбор стенда: работает ли
//! на этой машине DXGI Desktop Duplication. Наличие видеокарты его не решает —
//! дублирование поддерживает не карта, а адаптер, которому принадлежит рабочий
//! стол, и у него должен быть подключённый выход. Поэтому арендованная машина
//! с мощной картой может не отдать ни одного кадра, а обычный ноутбук отдаст.
//!
//! Запускается вручную и печатает результат человеку; в продуктовом пути не
//! участвует.

use screen_capture::{DxgiDesktopCapture, ScreenCapture};

/// Код возврата, по которому скрипт может отличить пригодную машину.
const EXIT_UNUSABLE: i32 = 1;

pub fn run() -> i32 {
    println!("ClassOS — проверка захвата экрана");
    println!("Сессия: {}", session_description());
    println!();

    let mut capture = match DxgiDesktopCapture::new() {
        Ok(capture) => capture,
        Err(error) => {
            println!("DXGI недоступен: {error}");
            println!();
            print_verdict(false, "Desktop Duplication не инициализируется");
            return EXIT_UNUSABLE;
        }
    };

    let displays = match capture.displays() {
        Ok(displays) => displays,
        Err(error) => {
            println!("не удалось перечислить дисплеи: {error}");
            print_verdict(false, "адаптеры есть, выходов нет");
            return EXIT_UNUSABLE;
        }
    };

    if displays.is_empty() {
        // Самый частый случай на арендованных GPU-машинах: карта видна, но
        // выходов у неё нет, потому что монитор к ней не подключён и
        // виртуального дисплея в системе тоже нет.
        println!("Дисплеев не найдено: ни один адаптер не имеет подключённого выхода.");
        print_verdict(false, "нет ни одного выхода для дублирования");
        return EXIT_UNUSABLE;
    }

    println!("Найдено дисплеев: {}", displays.len());
    for display in &displays {
        println!(
            "  id={} {}x{}{}",
            display.id,
            display.width,
            display.height,
            if display.primary {
                " (основной)"
            } else {
                ""
            }
        );
    }
    println!();

    // Перечисление ещё ничего не доказывает: DuplicateOutput отказывает
    // отдельно, поэтому проверяется получение настоящего кадра.
    if let Err(error) = capture.start(0) {
        println!("не удалось начать захват основного дисплея: {error}");
        print_verdict(false, "дисплей есть, дублирование отклонено");
        return EXIT_UNUSABLE;
    }

    let outcome = capture.next_frame();
    capture.stop();

    match outcome {
        Ok(frame) => {
            println!(
                "Кадр получен: {}x{}, {} байт.",
                frame.width,
                frame.height,
                frame.pixels.len()
            );
            print_verdict(true, "машина пригодна для T2–T4");
            0
        }
        Err(error) => {
            println!("кадр получить не удалось: {error}");
            print_verdict(false, "дублирование началось, но кадров нет");
            EXIT_UNUSABLE
        }
    }
}

/// Сессия, из которой запущена проверка.
///
/// Важна не меньше результата: RDP-сессия не является консольной, а Session
/// Host запускается только в консольную. Успешный захват в RDP не означает,
/// что агент сможет то же самое.
fn session_description() -> String {
    let current = std::process::id();
    match windows_platform::sessions::session_id_for_process(current) {
        Ok(session_id) => match windows_platform::sessions::active_console_session_id() {
            Some(console) if console == session_id => format!("{session_id} (консольная)"),
            Some(console) => format!("{session_id} — НЕ консольная, консольная сейчас {console}"),
            None => format!("{session_id}, активной консольной сессии нет"),
        },
        Err(error) => format!("не определена: {error}"),
    }
}

fn print_verdict(usable: bool, reason: &str) {
    println!();
    if usable {
        println!("ИТОГ: захват работает — {reason}.");
    } else {
        println!("ИТОГ: захват НЕ работает — {reason}.");
        println!();
        println!("Что обычно помогает:");
        println!("  * подключить монитор либо поставить драйвер виртуального дисплея;");
        println!("  * перевести карту из режима без вывода изображения в обычный;");
        println!("  * проверять из консольной сессии, а не из RDP.");
    }
}
