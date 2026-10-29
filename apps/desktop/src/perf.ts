import { commands } from "./bindings";
import { linesReceived } from "./log/useLog";
import { currentCore } from "./store";

// The main window's: its selection is its own state.
let showAllServices: (() => void) | null = null;
let isMeasuring = false;

/** How the measurement puts the Project's combined stream on screen; without it, no log is open. */
export function onPerfMeasurement(show: () => void) {
  showAllServices = show;
  if (isMeasuring) show();
}

/**
 * The log performance measurement (src-tauri/src/perf.rs): when the app was
 * launched for one, starts the chatty Project and records every frame while
 * its combined stream is on screen, then reports and quits.
 */
export async function runPerfMeasurement() {
  const settings = await commands.perfSettings();
  const core = currentCore();
  if (!settings || core.phase !== "ready") return;
  const project = core.view.projects[0];
  isMeasuring = true;
  showAllServices?.();
  await commands.startAll(project.id);

  // Let the first batch (the buffered lines) land before measuring.
  await new Promise((resolve) => setTimeout(resolve, 1_000));
  const firstLines = linesReceived();
  const frames: number[] = [];
  const start = performance.now();
  let last = start;
  await new Promise<void>((resolve) => {
    const tick = (now: number) => {
      frames.push(now - last);
      last = now;
      if (now - start < settings.seconds * 1_000) requestAnimationFrame(tick);
      else resolve();
    };
    requestAnimationFrame(tick);
  });

  const sorted = frames.toSorted((a, b) => a - b);
  const percentile = (p: number) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))];
  const round = (ms: number) => Math.round(ms * 10) / 10;
  await commands.finishPerfRun({
    seconds: round((last - start) / 1_000),
    linesReceived: linesReceived() - firstLines,
    frames: frames.length,
    frameMsP50: round(percentile(0.5)),
    frameMsP95: round(percentile(0.95)),
    frameMsMax: round(sorted[sorted.length - 1]),
    longFrames: frames.filter((ms) => ms > 33.4).length,
  });
}
