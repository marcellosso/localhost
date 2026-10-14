import { describe, expect, it } from "vitest";
import type { ServiceSuggestion } from "./bindings";
import { directoryLabel, suggestionDrafts, suggestionEdits, variableDrafts, variableEdits } from "./editors";

function suggestion(name: string, isRecommended: boolean, port: number | null = null): ServiceSuggestion {
  return {
    name,
    key: name.toLowerCase(),
    workingDirectory: ".",
    commandLine: `pnpm ${name.toLowerCase()}`,
    port,
    portCanChange: true,
    isRecommended,
    foundIn: "package.json",
    detail: `"${name.toLowerCase()}": vite`,
  };
}

describe("editors", () => {
  it("lists plain variables by name, then Secrets without their values", () => {
    const drafts = variableDrafts({ values: { MODE: "dev", API: "http://localhost" }, secretNames: ["TOKEN"] });
    expect(drafts.map(({ name, value, isSecret, isValueKnown }) => [name, value, isSecret, isValueKnown])).toEqual([
      ["API", "http://localhost", false, true],
      ["MODE", "dev", false, true],
      ["TOKEN", "", true, false],
    ]);
  });

  it("keeps a stored Secret's value unless it was typed or revealed, and drops empty rows", () => {
    const [mode, token, renamed, typed] = variableDrafts({ values: { MODE: "dev" }, secretNames: ["A", "B", "C"] });
    const empty = { ...mode, id: -1, name: "", value: "" };
    const edits = variableEdits([
      mode,
      token,
      { ...renamed, name: "B2" },
      { ...typed, value: "s3cret", isValueKnown: true },
      empty,
    ]);
    expect(edits).toEqual([
      { name: "MODE", value: { kind: "plain", value: "dev" } },
      { name: "A", value: { kind: "storedSecret", previousName: "A" } },
      { name: "B2", value: { kind: "storedSecret", previousName: "B" } },
      { name: "C", value: { kind: "secret", value: "s3cret" } },
    ]);
  });

  it("checks the likely Suggestions and adds only the checked ones, as edited", () => {
    const [dev, build] = suggestionDrafts([suggestion("Dev", true, 5173), suggestion("Build", false)]);
    expect([dev.isChecked, build.isChecked]).toEqual([true, false]);
    expect(dev.port).toBe("5173");
    expect(suggestionEdits([{ ...dev, name: "Web", key: "web" }, build])).toEqual([
      { name: "Web", key: "web", workingDirectory: ".", commandLine: "pnpm dev", port: "5173", portCanChange: true },
    ]);
  });

  it("shows working directories as relative paths", () => {
    expect(directoryLabel(".")).toBe("./");
    expect(directoryLabel("apps/web")).toBe("./apps/web");
  });
});
