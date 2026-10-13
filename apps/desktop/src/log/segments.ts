import type { LogLine, TextStyle } from "../bindings";

/** A run of a line's text drawn one way: its ANSI style, the URL it opens, whether it matches the filter. */
export type Segment = { text: string; style: TextStyle; url: string | null; isMatch: boolean };

/**
 * The line's text cut wherever its style changes, a local URL starts or
 * ends, or a match of `filter` (case-insensitive) starts or ends.
 */
export function segments(line: LogLine, filter: string): Segment[] {
  const text = line.spans.map((span) => span.text).join("");
  const cuts = new Set<number>([0, text.length]);
  let offset = 0;
  for (const span of line.spans) {
    cuts.add(offset);
    offset += span.text.length;
  }
  for (const link of line.links) {
    cuts.add(link.start);
    cuts.add(link.end);
  }
  const matches = matchRanges(text, filter);
  for (const [start, end] of matches) {
    cuts.add(start);
    cuts.add(end);
  }
  const points = [...cuts].sort((a, b) => a - b);
  const result: Segment[] = [];
  let spanIndex = 0;
  let spanEnd = line.spans[0]?.text.length ?? 0;
  for (let i = 0; i < points.length - 1; i++) {
    const [start, end] = [points[i], points[i + 1]];
    if (start === end) continue;
    while (start >= spanEnd && spanIndex < line.spans.length - 1) {
      spanIndex += 1;
      spanEnd += line.spans[spanIndex].text.length;
    }
    result.push({
      text: text.slice(start, end),
      style: line.spans[spanIndex].style,
      url: line.links.find((link) => link.start <= start && end <= link.end)?.url ?? null,
      isMatch: matches.some(([from, to]) => from <= start && end <= to),
    });
  }
  return result;
}

/** Whether the line (or its Service's prefix) contains `filter`, ignoring case. */
export function lineMatches(line: LogLine, filter: string, prefix?: string): boolean {
  if (!filter) return true;
  const needle = filter.toLowerCase();
  return (
    line.spans.map((span) => span.text).join("").toLowerCase().includes(needle) ||
    (prefix?.toLowerCase().includes(needle) ?? false)
  );
}

function matchRanges(text: string, filter: string): [number, number][] {
  if (!filter) return [];
  const haystack = text.toLowerCase();
  const needle = filter.toLowerCase();
  const ranges: [number, number][] = [];
  for (let at = haystack.indexOf(needle); at >= 0; at = haystack.indexOf(needle, at + needle.length)) {
    ranges.push([at, at + needle.length]);
  }
  return ranges;
}
