//! The handful of strings Rust has to own.
//!
//! Almost everything the user reads is drawn by the webview and lives in
//! `frontend/messages/`. Three things cannot be: the tray menu, which exists
//! before any window does, and the two overlays Rust injects as raw HTML. Those
//! are here.
//!
//! Deliberately a plain table rather than a second i18n framework. Nine strings
//! per language do not justify one, and the frontend catalogue is where a
//! translator should be spending their time.
//!
//! **When adding a language here, add it to `frontend/project.inlang/settings.json`
//! too.** `every_locale_is_complete` checks the two agree.

/// Every string Rust draws, for one language.
pub struct Strings {
    pub tray_open: &'static str,
    pub tray_quit: &'static str,

    pub locked_title: &'static str,
    /// Takes the reason (list name and time left) as `{reason}`.
    pub locked_body: &'static str,
    pub locked_ok: &'static str,

    pub extension_title: &'static str,
    /// Takes `{browser}` and `{store}`.
    pub extension_body: &'static str,
    /// Takes `{browser}`.
    pub extension_install: &'static str,
    pub extension_dismiss: &'static str,

    /// Shown instead of `extension_title` when the extension is installed
    /// and checking in, but has not been granted "Allow in Incognito" — a
    /// private window in that state has nothing blocking it at all.
    pub extension_incognito_title: &'static str,
    /// Takes `{browser}`.
    pub extension_incognito_body: &'static str,
    /// Takes `{browser}`. The button this labels opens the browser's own
    /// extension settings, not a store page — installing again does nothing.
    pub extension_incognito_action: &'static str,
}

const EN: Strings = Strings {
    tray_open: "Open Focuser",
    tray_quit: "Quit",
    locked_title: "Focuser is locked",
    locked_body: "You asked Focuser to stay running until this lock ends, so it will not quit yet: {reason}.",
    locked_ok: "OK",
    extension_title: "Extension required",
    extension_body: "Focuser closed {browser} because the Focuser browser extension is not installed. Install it from the {store} to keep using {browser} while blocks are active.",
    extension_install: "Install the extension for {browser}",
    extension_dismiss: "Dismiss",
    extension_incognito_title: "Allow the extension in Incognito",
    extension_incognito_body: "Focuser closed {browser} because its Incognito windows are not covered — the extension is installed, but is not allowed to run there. Turn on \"Allow in Incognito\" to keep using {browser} while blocks are active.",
    extension_incognito_action: "Open extension settings for {browser}",
};

const ES: Strings = Strings {
    tray_open: "Abrir Focuser",
    tray_quit: "Salir",
    locked_title: "Focuser está bloqueado",
    locked_body: "Pediste que Focuser siguiera en marcha hasta que terminara este bloqueo, así que todavía no se cerrará: {reason}.",
    locked_ok: "Aceptar",
    extension_title: "Se necesita la extensión",
    extension_body: "Focuser ha cerrado {browser} porque la extensión de Focuser no está instalada. Instálala desde {store} para seguir usando {browser} mientras los bloqueos estén activos.",
    extension_install: "Instalar la extensión para {browser}",
    extension_dismiss: "Descartar",
    extension_incognito_title: "Permite la extensión en Incógnito",
    extension_incognito_body: "Focuser ha cerrado {browser} porque sus ventanas de Incógnito no están cubiertas: la extensión está instalada, pero no tiene permiso para funcionar ahí. Activa \"Permitir en Incógnito\" para seguir usando {browser} mientras los bloqueos estén activos.",
    extension_incognito_action: "Abrir la configuración de la extensión para {browser}",
};

const FR: Strings = Strings {
    tray_open: "Ouvrir Focuser",
    tray_quit: "Quitter",
    locked_title: "Focuser est verrouillé",
    locked_body: "Vous avez demandé à Focuser de rester ouvert jusqu'à la fin de ce verrou, il ne quittera donc pas encore : {reason}.",
    locked_ok: "OK",
    extension_title: "Extension requise",
    extension_body: "Focuser a fermé {browser} parce que l'extension de navigateur Focuser n'est pas installée. Installez-la depuis {store} pour continuer à utiliser {browser} pendant que les blocages sont actifs.",
    extension_install: "Installer l'extension pour {browser}",
    extension_dismiss: "Ignorer",
    extension_incognito_title: "Autoriser l'extension en navigation privée",
    extension_incognito_body: "Focuser a fermé {browser} parce que ses fenêtres de navigation privée ne sont pas couvertes : l'extension est installée, mais n'est pas autorisée à s'y exécuter. Activez « Autoriser en navigation privée » pour continuer à utiliser {browser} pendant que les blocages sont actifs.",
    extension_incognito_action: "Ouvrir les paramètres de l'extension pour {browser}",
};

const DE: Strings = Strings {
    tray_open: "Focuser öffnen",
    tray_quit: "Beenden",
    locked_title: "Focuser ist gesperrt",
    locked_body: "Du hast Focuser gebeten, bis zum Ende dieser Sperre laufen zu bleiben, deshalb wird es noch nicht beendet: {reason}.",
    locked_ok: "OK",
    extension_title: "Erweiterung erforderlich",
    extension_body: "Focuser hat {browser} geschlossen, weil die Focuser-Browsererweiterung nicht installiert ist. Installiere sie über {store}, um {browser} weiter zu nutzen, während Sperren aktiv sind.",
    extension_install: "Erweiterung für {browser} installieren",
    extension_dismiss: "Schließen",
    extension_incognito_title: "Erweiterung im Inkognitomodus zulassen",
    extension_incognito_body: "Focuser hat {browser} geschlossen, weil Inkognitofenster nicht abgedeckt sind — die Erweiterung ist installiert, darf dort aber nicht laufen. Aktiviere „In Inkognitofenstern zulassen“, um {browser} weiter zu nutzen, während Sperren aktiv sind.",
    extension_incognito_action: "Erweiterungseinstellungen für {browser} öffnen",
};

const PT: Strings = Strings {
    tray_open: "Abrir o Focuser",
    tray_quit: "Sair",
    locked_title: "O Focuser está bloqueado",
    locked_body: "Você pediu para o Focuser continuar em execução até este bloqueio terminar, então ele ainda não vai sair: {reason}.",
    locked_ok: "OK",
    extension_title: "Extensão necessária",
    extension_body: "O Focuser fechou o {browser} porque a extensão de navegador do Focuser não está instalada. Instale pela {store} para continuar usando o {browser} enquanto os bloqueios estiverem ativos.",
    extension_install: "Instalar a extensão para o {browser}",
    extension_dismiss: "Dispensar",
    extension_incognito_title: "Permitir a extensão no modo anônimo",
    extension_incognito_body: "O Focuser fechou o {browser} porque as janelas anônimas não estão cobertas — a extensão está instalada, mas não tem permissão para funcionar nelas. Ative \"Permitir no modo anônimo\" para continuar usando o {browser} enquanto os bloqueios estiverem ativos.",
    extension_incognito_action: "Abrir as configurações da extensão para o {browser}",
};

const IT: Strings = Strings {
    tray_open: "Apri Focuser",
    tray_quit: "Esci",
    locked_title: "Focuser è bloccato",
    locked_body: "Hai chiesto a Focuser di restare in esecuzione fino alla fine di questo blocco, quindi non uscirà ancora: {reason}.",
    locked_ok: "OK",
    extension_title: "Estensione richiesta",
    extension_body: "Focuser ha chiuso {browser} perché l'estensione del browser non è installata. Installala da {store} per continuare a usare {browser} mentre i blocchi sono attivi.",
    extension_install: "Installa l'estensione per {browser}",
    extension_dismiss: "Ignora",
    extension_incognito_title: "Consenti l'estensione in Incognito",
    extension_incognito_body: "Focuser ha chiuso {browser} perché le finestre in incognito non sono coperte: l'estensione è installata, ma non è autorizzata a funzionare lì. Attiva \"Consenti in incognito\" per continuare a usare {browser} mentre i blocchi sono attivi.",
    extension_incognito_action: "Apri le impostazioni dell'estensione per {browser}",
};

const RU: Strings = Strings {
    tray_open: "Открыть Focuser",
    tray_quit: "Выход",
    locked_title: "Focuser заблокирован",
    locked_body: "Вы попросили Focuser не закрываться до конца блокировки, поэтому он пока не выйдет: {reason}.",
    locked_ok: "ОК",
    extension_title: "Требуется расширение",
    extension_body: "Focuser закрыл {browser}, потому что расширение Focuser для браузера не установлено. Установите его из {store}, чтобы продолжать пользоваться {browser} при активных блокировках.",
    extension_install: "Установить расширение для {browser}",
    extension_dismiss: "Закрыть",
    extension_incognito_title: "Разрешить расширение в режиме инкогнито",
    extension_incognito_body: "Focuser закрыл {browser}, потому что окна в режиме инкогнито не защищены — расширение установлено, но ему не разрешено там работать. Включите «Разрешить в режиме инкогнито», чтобы продолжать пользоваться {browser} при активных блокировках.",
    extension_incognito_action: "Открыть настройки расширения для {browser}",
};

const ZH: Strings = Strings {
    tray_open: "打开 Focuser",
    tray_quit: "退出",
    locked_title: "Focuser 已锁定",
    locked_body: "你要求 Focuser 在这次锁定结束前保持运行，所以它暂时不会退出：{reason}。",
    locked_ok: "好",
    extension_title: "需要安装扩展",
    extension_body: "Focuser 关闭了 {browser}，因为没有安装 Focuser 浏览器扩展。请从{store}安装，以便在拦截生效期间继续使用 {browser}。",
    extension_install: "为 {browser} 安装扩展",
    extension_dismiss: "忽略",
    extension_incognito_title: "允许扩展在隐身模式下运行",
    extension_incognito_body: "Focuser 关闭了 {browser}，因为它的隐身窗口没有被覆盖——扩展已安装，但不允许在隐身模式下运行。请开启\"在隐身模式下启用\"，以便在拦截生效期间继续使用 {browser}。",
    extension_incognito_action: "打开 {browser} 的扩展设置",
};

const JA: Strings = Strings {
    tray_open: "Focuser を開く",
    tray_quit: "終了",
    locked_title: "Focuser はロックされています",
    locked_body: "このロックが終わるまで Focuser を起動したままにするよう設定されているため、まだ終了しません: {reason}。",
    locked_ok: "OK",
    extension_title: "拡張機能が必要です",
    extension_body: "Focuser 拡張機能がインストールされていないため、Focuser が {browser} を終了しました。ブロック中も {browser} を使い続けるには、{store} からインストールしてください。",
    extension_install: "{browser} 用の拡張機能をインストール",
    extension_dismiss: "閉じる",
    extension_incognito_title: "シークレットモードで拡張機能を許可",
    extension_incognito_body: "シークレットウィンドウがカバーされていないため、Focuser が {browser} を終了しました。拡張機能はインストール済みですが、そこで動作する許可がありません。ブロック中も {browser} を使い続けるには、「シークレットモードでの実行を許可する」をオンにしてください。",
    extension_incognito_action: "{browser} の拡張機能の設定を開く",
};

const KO: Strings = Strings {
    tray_open: "Focuser 열기",
    tray_quit: "종료",
    locked_title: "Focuser가 잠겨 있습니다",
    locked_body: "이 잠금이 끝날 때까지 Focuser를 계속 실행하도록 설정해 두었기 때문에 아직 종료되지 않습니다: {reason}.",
    locked_ok: "확인",
    extension_title: "확장 프로그램이 필요합니다",
    extension_body: "Focuser 브라우저 확장 프로그램이 설치되어 있지 않아 Focuser가 {browser}을(를) 종료했습니다. 차단이 켜져 있는 동안에도 {browser}을(를) 쓰려면 {store}에서 설치해 주세요.",
    extension_install: "{browser}용 확장 프로그램 설치",
    extension_dismiss: "닫기",
    extension_incognito_title: "시크릿 모드에서 확장 프로그램 허용",
    extension_incognito_body: "시크릿 창이 보호되지 않아 Focuser가 {browser}을(를) 종료했습니다. 확장 프로그램은 설치되어 있지만 그곳에서 실행할 권한이 없습니다. 차단이 켜져 있는 동안에도 {browser}을(를) 계속 쓰려면 \"시크릿 모드에서 허용\"을 켜 주세요.",
    extension_incognito_action: "{browser} 확장 프로그램 설정 열기",
};

/// Strings for `locale`, falling back to English for anything we do not ship.
pub fn strings(locale: &str) -> &'static Strings {
    match locale {
        "es" => &ES,
        "fr" => &FR,
        "de" => &DE,
        "pt" => &PT,
        "it" => &IT,
        "ru" => &RU,
        "zh" => &ZH,
        "ja" => &JA,
        "ko" => &KO,
        _ => &EN,
    }
}

/// The language the user chose, or English.
///
/// Read per use rather than cached: it is a single indexed row, and a cache
/// would need invalidating from the settings page for no real gain.
pub fn saved_locale(db: &focuser_core::db::Database) -> String {
    db.get_setting("language")
        .ok()
        .flatten()
        .unwrap_or_else(|| "en".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_locale_falls_back_to_english() {
        assert_eq!(strings("qq").tray_quit, EN.tray_quit);
        assert_eq!(strings("").tray_quit, EN.tray_quit);
    }

    #[test]
    fn spanish_is_actually_spanish() {
        assert_eq!(strings("es").tray_quit, "Salir");
        assert_ne!(strings("es").locked_title, EN.locked_title);
    }

    /// Every placeholder the English string uses must survive translation, or
    /// the sentence renders with a hole in it.
    #[test]
    fn placeholders_match_across_locales() {
        for locale in ["es", "fr", "de", "pt", "it", "ru", "zh", "ja", "ko"] {
            let t = strings(locale);
            assert!(t.locked_body.contains("{reason}"), "{locale} locked_body");
            assert!(t.extension_body.contains("{browser}"), "{locale} browser");
            assert!(t.extension_body.contains("{store}"), "{locale} store");
            assert!(
                t.extension_install.contains("{browser}"),
                "{locale} install"
            );
            assert!(
                t.extension_incognito_body.contains("{browser}"),
                "{locale} incognito body"
            );
            assert!(
                t.extension_incognito_action.contains("{browser}"),
                "{locale} incognito action"
            );
        }
    }

    /// The Rust table and the frontend catalogue must ship the same languages,
    /// or the tray sits in English while the window is in Spanish.
    #[test]
    fn every_locale_is_complete() {
        let settings = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("frontend/project.inlang/settings.json");
        let raw = std::fs::read_to_string(&settings).expect("inlang settings are missing");
        let parsed: serde_json::Value = serde_json::from_str(&raw).expect("settings are not JSON");
        let frontend: Vec<String> = parsed["locales"]
            .as_array()
            .expect("no locales array")
            .iter()
            .map(|v| v.as_str().unwrap_or_default().to_string())
            .collect();

        for locale in &frontend {
            let table = strings(locale);
            assert_ne!(
                table.tray_quit, "",
                "{locale} is in the frontend catalogue but has no Rust strings"
            );
            if locale != "en" {
                assert_ne!(
                    table.tray_quit, EN.tray_quit,
                    "{locale} falls back to English in i18n.rs — add it to the table"
                );
            }
        }
    }
}
