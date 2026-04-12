"""
Build Chrome and Firefox extension packages from the source extension/ folder.
Run from the repo root: python extension/build.py
"""

import json
import os
import shutil
import zipfile

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
DIST_DIR = os.path.join(SCRIPT_DIR, "dist")
CHROME_DIR = os.path.join(DIST_DIR, "chrome")
FIREFOX_DIR = os.path.join(DIST_DIR, "firefox")

SHARED_FILES = [
    "background.js",
    "blocked.css",
    "blocked.html",
    "blocked.js",
    "content-block.js",
    "content-early.js",
    "content-hide.css",
    "messages.json",
    "popup.css",
    "popup.html",
    "popup.js",
]

ICON_FILES = ["icon16.png", "icon32.png", "icon48.png", "icon96.png", "icon128.png"]


def load_manifest():
    with open(os.path.join(SCRIPT_DIR, "manifest.json")) as f:
        return json.load(f)


def build_chrome_manifest(base):
    m = dict(base)
    m["background"] = {"service_worker": "background.js"}
    m.pop("browser_specific_settings", None)
    return m


def build_firefox_manifest(base):
    m = dict(base)
    m["background"] = {"scripts": ["background.js"]}
    return m


def copy_shared(dest):
    os.makedirs(dest, exist_ok=True)
    os.makedirs(os.path.join(dest, "icons"), exist_ok=True)
    for f in SHARED_FILES:
        src = os.path.join(SCRIPT_DIR, f)
        if os.path.exists(src):
            shutil.copy2(src, os.path.join(dest, f))
    for f in ICON_FILES:
        src = os.path.join(SCRIPT_DIR, "icons", f)
        if os.path.exists(src):
            shutil.copy2(src, os.path.join(dest, "icons", f))


def make_zip(src_dir, zip_path):
    if os.path.exists(zip_path):
        os.remove(zip_path)
    with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
        for root, _, files in os.walk(src_dir):
            for f in sorted(files):
                filepath = os.path.join(root, f)
                arcname = os.path.relpath(filepath, src_dir).replace("\\", "/")
                zf.write(filepath, arcname)


def main():
    base = load_manifest()
    version = base.get("version", "0.0.0")

    print(f"Building extension v{version}")

    for d in [CHROME_DIR, FIREFOX_DIR]:
        if os.path.exists(d):
            shutil.rmtree(d)

    copy_shared(CHROME_DIR)
    chrome_manifest = build_chrome_manifest(base)
    with open(os.path.join(CHROME_DIR, "manifest.json"), "w") as f:
        json.dump(chrome_manifest, f, indent=2)
    print(f"  Chrome: {CHROME_DIR}")

    copy_shared(FIREFOX_DIR)
    firefox_manifest = build_firefox_manifest(base)
    with open(os.path.join(FIREFOX_DIR, "manifest.json"), "w") as f:
        json.dump(firefox_manifest, f, indent=2)
    print(f"  Firefox: {FIREFOX_DIR}")

    chrome_zip = os.path.join(DIST_DIR, f"focuser-chrome-{version}.zip")
    firefox_zip = os.path.join(DIST_DIR, f"focuser-firefox-{version}.zip")
    make_zip(CHROME_DIR, chrome_zip)
    make_zip(FIREFOX_DIR, firefox_zip)

    print(f"\n  {chrome_zip} ({os.path.getsize(chrome_zip)} bytes)")
    print(f"  {firefox_zip} ({os.path.getsize(firefox_zip)} bytes)")
    print("\nDone.")


if __name__ == "__main__":
    main()
