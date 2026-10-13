import type { LogBatch, LogLine } from "../bindings";

/** The most lines a log keeps, as the core's buffer of one Run. */
export const MAX_LINES = 10_000;

/**
 * The lines a log shows, oldest first. Appending mutates in place (copying
 * 10,000 lines per frame would be the bottleneck); `version` says it changed.
 * Trimming waits for 1,000 extra lines so it isn't done on every batch.
 */
export class LogBuffer {
  lines: LogLine[] = [];
  /** Lines dropped from the front so far, to keep row keys stable. */
  dropped = 0;
  version = 0;
  isFinished = false;

  append(batch: LogBatch) {
    if (batch.skipped > 0 || batch.lines.length >= MAX_LINES) {
      // The burst replaces everything shown.
      this.dropped += this.lines.length + batch.skipped;
      this.lines = batch.lines.slice(-MAX_LINES);
    } else {
      for (const line of batch.lines) this.lines.push(line);
      if (this.lines.length > MAX_LINES + 1_000) {
        const excess = this.lines.length - MAX_LINES;
        this.lines.splice(0, excess);
        this.dropped += excess;
      }
    }
    this.isFinished ||= batch.isFinished;
    this.version += 1;
  }
}
