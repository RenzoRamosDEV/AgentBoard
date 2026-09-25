import { describe, expect, it } from "vitest";
import { modelName } from "../format";

describe("modelName", () => {
  it("acorta modelos de Claude", () => {
    expect(modelName("claude-opus-5-5")).toBe("Opus 5.5");
    expect(modelName("claude-sonnet-4-5")).toBe("Sonnet 4.5");
    expect(modelName("claude-fable-5-1")).toBe("Fable 5.1");
    expect(modelName("claude-haiku-4-5")).toBe("Haiku 4.5");
  });
  it("deja intactos los demás", () => {
    expect(modelName("gpt-5-codex")).toBe("gpt-5-codex");
    expect(modelName("gemini-2.5-pro")).toBe("gemini-2.5-pro");
    expect(modelName("big-pickle")).toBe("big-pickle");
  });
});
