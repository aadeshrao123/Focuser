/**
 * Fills `.env.submit` by asking for each credential.
 *
 * Beats editing the file by hand: the values go from the prompt straight into a
 * gitignored file, never through a shell argument that lands in history.
 *
 *   npm run setup:submit
 */
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { createInterface } from "node:readline/promises";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const template = resolve(here, "../.env.submit.example");
const target = resolve(here, "../.env.submit");

const FIELDS = [
  { key: "CHROME_CLIENT_ID", label: "Chrome OAuth client ID" },
  { key: "CHROME_CLIENT_SECRET", label: "Chrome OAuth client secret" },
  { key: "FIREFOX_JWT_ISSUER", label: "Firefox JWT issuer (user:12345:67)" },
  { key: "FIREFOX_JWT_SECRET", label: "Firefox JWT secret" },
];

if (!existsSync(template)) {
  console.error(`Missing ${template}`);
  process.exit(1);
}

// One interface for the whole run. Opening a fresh one per question closes
// stdin after the first answer.
const rl = createInterface({ input: process.stdin, output: process.stdout });

try {
  if (existsSync(target)) {
    const overwrite = await rl.question(".env.submit already exists. Overwrite? (y/N) ");
    if (overwrite.trim().toLowerCase() !== "y") {
      console.log("Left it alone.");
      process.exit(0);
    }
  }

  let contents = readFileSync(template, "utf8");
  console.log("\nPaste each value.\n");

  for (const { key, label } of FIELDS) {
    const value = (await rl.question(`${label}: `)).trim();
    if (!value) {
      console.error(`\n${key} cannot be empty.`);
      process.exit(1);
    }
    contents = contents.replace(new RegExp(`^${key}=.*$`, "m"), `${key}=${value}`);
  }

  writeFileSync(target, contents, "utf8");
} finally {
  rl.close();
}

console.log(`
Wrote .env.submit (gitignored).

CHROME_REFRESH_TOKEN is still a placeholder. Generate it with:

  npx publish-extension init

Choose Chrome, answer yes to "Generate new refresh token?", approve in the
browser, then paste the code back.
`);
