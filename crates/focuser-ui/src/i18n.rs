//! The handful of strings Rust has to own.
//!
//! Almost everything the user reads is drawn by the webview and lives in
//! `frontend/messages/`. Three things cannot be: the tray menu, which exists
//! before any window does, and the two overlays Rust injects as raw HTML. Those
//! are here.
//!
//! Deliberately a plain table rather than a second i18n framework. Nine strings
//! do not justify one, and the frontend catalogue is where a translator should
//! be spending their time.
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
};

/// Strings for `locale`, falling back to English for anything we do not ship.
pub fn strings(locale: &str) -> &'static Strings {
    match locale {
        "es" => &ES,
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
        for locale in ["es"] {
            let t = strings(locale);
            assert!(t.locked_body.contains("{reason}"), "{locale} locked_body");
            assert!(t.extension_body.contains("{browser}"), "{locale} browser");
            assert!(t.extension_body.contains("{store}"), "{locale} store");
            assert!(
                t.extension_install.contains("{browser}"),
                "{locale} install"
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
