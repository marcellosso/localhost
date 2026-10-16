import { describe, expect, it } from "vitest";
import { keepsWebviewMenu } from "./webview";

describe("keepsWebviewMenu", () => {
  it("keeps the webview's menu in fields and on selected text only", () => {
    const field = document.createElement("input");
    const row = document.createElement("div");
    row.textContent = "npm run dev";
    document.body.append(field, row);
    const selection = document.getSelection()!;
    selection.removeAllRanges();

    expect(keepsWebviewMenu(field, selection)).toBe(true);
    expect(keepsWebviewMenu(row, selection)).toBe(false);
    selection.selectAllChildren(row);
    expect(keepsWebviewMenu(row, selection)).toBe(true);
    selection.removeAllRanges();
    document.body.replaceChildren();
  });
});
