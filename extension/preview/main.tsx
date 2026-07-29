/**
 * Renders the extension's pages in an ordinary browser tab.
 *
 * The block page and popup can otherwise only be seen by loading the built
 * extension into a real browser and getting yourself blocked, which is a slow
 * way to check a padding change.
 *
 * One state at a time, at full height: an earlier version stacked every case
 * down one scrolling page, which is not how anyone ever sees this. The picker
 * on the right switches language, so every translation can be looked at without
 * changing the browser's own setting.
 *
 *   npm run preview   →   http://localhost:5199
 */
import { install } from "./stubs";

// The app is imported dynamically because the stubs have to be in place first;
// a static import would be hoisted above this call.
install();
import("./app");
