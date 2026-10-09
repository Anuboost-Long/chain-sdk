import { desktop } from "@chain/sdk";

/** Long enough to read as a launch screen rather than a flicker. */
const MIN_VISIBLE_MS = 700;

const launchScreen = document.getElementById("launch-screen");

const windowShown = new Promise<void>((resolve) => {
  desktop.window
    .isShown()
    .catch(() => true)
    .then((shown) => (shown ? resolve() : desktop.window.onShown(resolve)));
});

/** The progress line moves only once the window is visible, not while it waits to show. */
const visibleSince = windowShown.then(() => {
  launchScreen?.toggleAttribute("data-window-shown", true);
  return performance.now();
});

export async function dismissLaunchScreen(): Promise<void> {
  if (!launchScreen) return;
  const shownFor = performance.now() - (await visibleSince);
  await new Promise((resolve) => setTimeout(resolve, MIN_VISIBLE_MS - shownFor));
  launchScreen.toggleAttribute("data-done", true);
  launchScreen.addEventListener("transitionend", () => launchScreen.remove(), { once: true });
  // No transition runs with reduced motion.
  setTimeout(() => launchScreen.remove(), 400);
}
