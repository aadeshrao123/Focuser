# Translating Focuser

Focuser ships in English and Spanish. If you speak another language well, you can add it,
and you will do a better job of it than machine translation or than the maintainer guessing.

**The Spanish is a first draft and has not been reviewed by a native speaker.** If you are
one, corrections are more welcome than a new language.

You do not need to know Rust. You do not need to run the app. Editing one JSON file is
enough.

## Adding a language

**1. Copy the English file.**

```
crates/focuser-ui/frontend/messages/en.json  →  messages/<your-locale>.json
```

Use a plain language code: `es`, `zh`, `de`, `fr`, `pt`. Only add a region (`pt-BR`) when
the difference actually matters.

**2. Register it** in `crates/focuser-ui/frontend/project.inlang/settings.json`:

```json
"locales": ["en", "es"]
```

**3. Translate the values, never the keys.**

```json
{
  "websites_title": "Sitios web",
  "websites_description": "Dominios, palabras clave y patrones de URL para bloquear."
}
```

`websites_title` stays exactly as it is. Only the text on the right changes.

**4. Open a pull request.** That is the whole job.

## The rules that matter

**Keep every placeholder.** `{count}`, `{domain}` and friends are replaced with real values
at runtime. A translation that loses one renders a sentence with a hole in it.

```json
"import_added": "Importadas {count} reglas."     ✅ keeps {count}
"import_added": "Reglas importadas."             ❌ the number vanishes
```

**Keep every key.** A missing key breaks the build, so nothing half-translated can ship.
If you cannot translate something, leave the English text as the value and open the PR
anyway — someone else can finish it.

**Plurals use a block, not a sentence.** English has two forms; many languages have more.
Write one line per form your language actually uses:

```json
"import_added": [
  {
    "declarations": ["input count", "local countPlural = count: plural"],
    "selectors": ["countPlural"],
    "match": {
      "countPlural=one": "Importada {count} regla.",
      "countPlural=few": "Importadas {count} reglas.",
      "countPlural=other": "Importadas {count} reglas."
    }
  }
]
```

The valid forms for your language are decided by `Intl.PluralRules`, not by us. Polish uses
`one`, `few`, `many`, `other`. Japanese uses only `other`. Use what your language needs.

## Tone

Focuser talks to someone who is trying not to open a website, and who is annoyed at it for
stopping them. The English copy is deliberately plain, slightly dry, and never smug or
preachy. Please keep that. A block page that lectures gets uninstalled.

## Checking your work

If you have Node installed:

```bash
cd crates/focuser-ui/frontend
npm install
npm run paraglide   # fails loudly if the file is malformed
npx tsc --noEmit    # fails if a key is missing
```

If you do not, open the PR anyway. CI runs exactly these checks and will tell us both.

## The browser extension

The extension has its own catalogue, in YAML rather than JSON:

```
extension/locales/en.yml  →  extension/locales/<your-locale>.yml
```

Same rules: translate the values, keep every `{placeholder}`, keep every key. No
registration step, the file being there is enough.

Plurals look like this, and the count arrives as `$1`:

```yml
rulesActive:
  1: 1 regla activa
  n: $1 reglas activas
```

`@wxt-dev/i18n` only supports `1` and `n`. If your language needs `few` or `many`, say so
in the pull request and we will work out what to do.

**The extension follows the browser's language, not the app's setting.** That is a limit
of the `browser.i18n` API, which has no way to change locale while running. So a Spanish
browser gets a Spanish extension even if the app is set to English. The two catalogues are
independent and you are welcome to do one without the other.

## Credit

Translators are contributors. You will show up in the repository's contributor list, and
you are welcome to add yourself to the language list in the pull request.
