// A unified diff of one file before and after an edit. The editors change one
// stretch of a file, so the lines that differ are found between the common
// start and end, which stays fast on a large `~/.claude.json`.

const CONTEXT = 3;

/** `before` → `after` as unified-diff lines, `null` meaning no file. */
export function diff(file, before, after) {
  const old = split(before);
  const now = split(after);
  let start = 0;
  while (start < old.length && start < now.length && old[start] === now[start]) start += 1;
  let end = 0;
  while (end < old.length - start && end < now.length - start && old[old.length - 1 - end] === now[now.length - 1 - end]) end += 1;
  const from = Math.max(0, start - CONTEXT);
  const oldTo = Math.min(old.length, old.length - end + CONTEXT);
  const nowTo = Math.min(now.length, now.length - end + CONTEXT);
  const lines = [
    `--- ${before === null ? '/dev/null' : file}`,
    `+++ ${after === null ? '/dev/null' : file}`,
    `@@ -${range(from, oldTo - from)} +${range(from, nowTo - from)} @@`,
  ];
  for (let index = from; index < start; index += 1) lines.push(` ${old[index]}`);
  for (let index = start; index < old.length - end; index += 1) lines.push(`-${old[index]}`);
  for (let index = start; index < now.length - end; index += 1) lines.push(`+${now[index]}`);
  for (let index = now.length - end; index < nowTo; index += 1) lines.push(` ${now[index]}`);
  return lines;
}

function split(text) {
  if (text === null || text === '') return [];
  return text.replace(/\n$/, '').split('\n');
}

function range(start, length) {
  return length === 0 ? `${start},0` : `${start + 1},${length}`;
}
