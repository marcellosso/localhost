import { describe, expect, it } from "vitest";
import type { ServiceSuggestion } from "./bindings";
import {
  directoryLabel,
  draftStartsAfter,
  mergingDrafts,
  splittingDraft,
  suggestionDrafts,
  suggestionEdits,
  variableDrafts,
  variableEdits,
} from "./editors";

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
      {
        name: "Web",
        key: "web",
        workingDirectory: ".",
        commandLine: "pnpm dev",
        port: "5173",
        portCanChange: true,
        dependencyKeys: [],
      },
    ]);
  });

  it("saves Starts after as the Keys of the checked drafts only", () => {
    const [api, infra, docs] = suggestionDrafts([suggestion("API", true), suggestion("Infra", true), suggestion("Docs", false)]);
    const edits = suggestionEdits([{ ...api, startsAfter: [infra.id, docs.id] }, { ...infra, key: "containers" }, docs]);
    expect(edits.map((edit) => [edit.key, edit.dependencyKeys])).toEqual([
      ["api", ["containers"]],
      ["containers", []],
    ]);
  });

  it("says when a draft already starts after another, through others", () => {
    const [web, api, infra] = suggestionDrafts([suggestion("Web", true), suggestion("API", true), suggestion("Infra", true)]);
    const drafts = [
      { ...web, startsAfter: [api.id] },
      { ...api, startsAfter: [infra.id] },
      infra,
    ];
    expect(draftStartsAfter(drafts, drafts[0], infra)).toBe(true);
    expect(draftStartsAfter(drafts, infra, drafts[0])).toBe(false);
  });

  it("merges drafts into one where the first was, and splits it back", () => {
    const [api, postgres, temporal] = suggestionDrafts([
      suggestion("API", true),
      { ...suggestion("postgres", false), composeServices: ["postgres"] },
      { ...suggestion("temporal", false), composeServices: ["temporal"] },
    ]);
    const before = [{ ...api, startsAfter: [postgres.id, temporal.id] }, postgres, temporal];
    const merged = { ...suggestion("Infra", true), commandLine: "docker compose up postgres temporal" };
    const after = mergingDrafts(before, [postgres.id, temporal.id], merged);
    expect(after.map((draft) => [draft.name, draft.isChecked])).toEqual([
      ["API", true],
      ["Infra", true],
    ]);
    expect(after[0].startsAfter).toEqual([after[1].id]);

    const split = splittingDraft(after, after[1].id);
    expect(split.map((draft) => [draft.name, draft.isChecked])).toEqual([
      ["API", true],
      ["postgres", true],
      ["temporal", true],
    ]);
    expect(split[0].startsAfter).toEqual([postgres.id, temporal.id]);
  });

  it("splits a whole Compose file into the drafts for its containers", () => {
    const [infra, postgres, temporal, api] = suggestionDrafts([
      { ...suggestion("Infra", true), composeServices: ["postgres", "temporal"], foundIn: "compose.yaml" },
      { ...suggestion("postgres", false), composeServices: ["postgres"], foundIn: "compose.yaml" },
      { ...suggestion("temporal", false), composeServices: ["temporal"], foundIn: "compose.yaml" },
      suggestion("API", true),
    ]);
    const split = splittingDraft([infra, postgres, temporal, { ...api, startsAfter: [infra.id] }], infra.id);
    expect(split.map((draft) => draft.isChecked)).toEqual([false, true, true, true]);
    expect(split[3].startsAfter).toEqual([postgres.id, temporal.id]);
  });

  it("shows working directories as relative paths", () => {
    expect(directoryLabel(".")).toBe("./");
    expect(directoryLabel("apps/web")).toBe("./apps/web");
  });
});
