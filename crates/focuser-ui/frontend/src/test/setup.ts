import "@testing-library/jest-dom/vitest";
import { beforeEach } from "vitest";

/**
 * A `localStorage` for tests.
 *
 * Paraglide resolves the locale from it on every message call, so without one
 * every render throws before it draws anything. jsdom does not reliably expose
 * it here, and a Map is enough for what we ask of it.
 */
class MemoryStorage implements Storage {
  #items = new Map<string, string>();

  get length() {
    return this.#items.size;
  }
  clear() {
    this.#items.clear();
  }
  getItem(key: string) {
    return this.#items.get(key) ?? null;
  }
  key(index: number) {
    return [...this.#items.keys()][index] ?? null;
  }
  removeItem(key: string) {
    this.#items.delete(key);
  }
  setItem(key: string, value: string) {
    this.#items.set(key, String(value));
  }
}

const storage = new MemoryStorage();
for (const target of [globalThis, globalThis.window].filter(Boolean)) {
  Object.defineProperty(target, "localStorage", { value: storage, configurable: true });
}

// A locale left over from one test must not decide what the next one renders.
beforeEach(() => storage.clear());
