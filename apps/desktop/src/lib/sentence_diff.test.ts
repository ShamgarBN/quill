/**
 * Tests for the sentence-level diff that drives the Phase 6.3 track-changes
 * review UI. These cover the splitter (where most bugs hide), the LCS
 * back-trace, the Replace coalescing, and the decision-applier round-trip.
 */
import { describe, expect, it } from "vitest";
import {
  applyDecisions,
  diffSentences,
  joinSentences,
  splitSentences,
  type DiffChunk,
} from "./sentence_diff";

describe("splitSentences", () => {
  it("returns empty for empty input", () => {
    expect(splitSentences("")).toEqual([]);
  });

  it("splits simple sentences and preserves trailing spaces", () => {
    const result = splitSentences("Hello world. This is fine. End.");
    expect(result.map((s) => s.text)).toEqual([
      "Hello world.",
      "This is fine.",
      "End.",
    ]);
    expect(result[0]?.trailing).toBe(" ");
    expect(result[1]?.trailing).toBe(" ");
    expect(result[2]?.trailing).toBe("");
  });

  it("handles question marks and exclamation marks", () => {
    expect(splitSentences("Run! Now? Yes.").map((s) => s.text)).toEqual([
      "Run!",
      "Now?",
      "Yes.",
    ]);
  });

  it("does not split on common abbreviations", () => {
    expect(splitSentences("Mr. Smith arrived. He waved.").map((s) => s.text)).toEqual([
      "Mr. Smith arrived.",
      "He waved.",
    ]);
    expect(
      splitSentences("She read St. Augustine. Then she left.").map((s) => s.text),
    ).toEqual(["She read St. Augustine.", "Then she left."]);
  });

  it("anchors paragraph breaks to the preceding sentence", () => {
    const text = "First paragraph here.\n\nSecond paragraph.";
    const result = splitSentences(text);
    expect(result.map((s) => s.text)).toEqual([
      "First paragraph here.",
      "Second paragraph.",
    ]);
    expect(result[0]?.trailing).toBe("\n\n");
    // Round-trip should recompose the original.
    expect(joinSentences(result)).toBe(text);
  });

  it("keeps a closing quote with the sentence", () => {
    const text = 'She said, "Run." Then she vanished.';
    expect(splitSentences(text).map((s) => s.text)).toEqual([
      'She said, "Run."',
      "Then she vanished.",
    ]);
  });

  it("round-trips arbitrary prose verbatim", () => {
    const samples = [
      "A single sentence.",
      "Two. Sentences.",
      "Para one.\n\nPara two.\n\nPara three.",
      "Mr. Foo said, \"Hello.\" She replied, 'Hi.'",
      "An unterminated fragment",
      "Mid-line\nnewline without paragraph break still works.",
    ];
    for (const s of samples) {
      expect(joinSentences(splitSentences(s))).toBe(s);
    }
  });
});

describe("diffSentences", () => {
  it("returns a single equal chunk for identical text", () => {
    const chunks = diffSentences("Hello world.", "Hello world.");
    expect(chunks).toHaveLength(1);
    expect(chunks[0]?.op).toBe("equal");
  });

  it("returns an all-insert chunk when original is empty", () => {
    const chunks = diffSentences("", "New text appears.");
    expect(chunks).toHaveLength(1);
    expect(chunks[0]?.op).toBe("insert");
    expect(chunks[0]?.candidate.map((s) => s.text)).toEqual(["New text appears."]);
  });

  it("returns an all-delete chunk when candidate is empty", () => {
    const chunks = diffSentences("Goodbye world.", "");
    expect(chunks).toHaveLength(1);
    expect(chunks[0]?.op).toBe("delete");
  });

  it("coalesces adjacent (delete, insert) into a single replace chunk", () => {
    const chunks = diffSentences(
      "Same start. Old middle. Same end.",
      "Same start. New middle. Same end.",
    );
    const ops = chunks.map((c) => c.op);
    // equal + replace + equal — NOT equal + delete + insert + equal.
    expect(ops).toEqual(["equal", "replace", "equal"]);
    const replace = chunks[1];
    expect(replace?.original.map((s) => s.text)).toEqual(["Old middle."]);
    expect(replace?.candidate.map((s) => s.text)).toEqual(["New middle."]);
  });

  it("detects pure insertion in the middle", () => {
    const chunks = diffSentences("First. Last.", "First. Middle. Last.");
    expect(chunks.map((c) => c.op)).toEqual(["equal", "insert", "equal"]);
    expect(chunks[1]?.candidate.map((s) => s.text)).toEqual(["Middle."]);
  });

  it("detects pure deletion in the middle", () => {
    const chunks = diffSentences("First. Middle. Last.", "First. Last.");
    expect(chunks.map((c) => c.op)).toEqual(["equal", "delete", "equal"]);
    expect(chunks[1]?.original.map((s) => s.text)).toEqual(["Middle."]);
  });
});

describe("applyDecisions", () => {
  const chunks = (a: string, b: string): DiffChunk[] => diffSentences(a, b);

  it("accepting all replace chunks yields the candidate", () => {
    const a = "One. Two. Three.";
    const b = "One. Twenty. Three.";
    const c = chunks(a, b);
    const decisions = c.map(() => "accepted" as const);
    expect(applyDecisions(c, decisions)).toBe(b);
  });

  it("rejecting all changes yields the original", () => {
    const a = "Alpha. Beta. Gamma.";
    const b = "Alpha. Delta. Epsilon.";
    const c = chunks(a, b);
    const decisions = c.map((ch) => (ch.op === "equal" ? "accepted" : "rejected"));
    expect(applyDecisions(c, decisions)).toBe(a);
  });

  it("mixed decisions produce a partial accept", () => {
    const a = "Keep me. Replace me. Keep me too.";
    const b = "Keep me. Better version. Keep me too.";
    const c = chunks(a, b);
    expect(c.map((ch) => ch.op)).toEqual(["equal", "replace", "equal"]);
    // Reject the middle.
    const decisions = ["accepted", "rejected", "accepted"] as const;
    expect(applyDecisions(c, [...decisions])).toBe(a);
    // Accept the middle.
    const accept = ["accepted", "accepted", "accepted"] as const;
    expect(applyDecisions(c, [...accept])).toBe(b);
  });

  it("inserts are dropped on rejection, included on acceptance", () => {
    const c = chunks("Bookends.", "Bookends. New middle.");
    expect(c.map((ch) => ch.op)).toEqual(["equal", "insert"]);
    expect(applyDecisions(c, ["accepted", "rejected"])).toBe("Bookends.");
    expect(applyDecisions(c, ["accepted", "accepted"])).toBe("Bookends. New middle.");
  });

  it("deletes are kept on rejection, removed on acceptance", () => {
    const c = chunks("Keep. Remove. Keep more.", "Keep. Keep more.");
    expect(c.map((ch) => ch.op)).toEqual(["equal", "delete", "equal"]);
    expect(applyDecisions(c, ["accepted", "rejected", "accepted"])).toBe(
      "Keep. Remove. Keep more.",
    );
    expect(applyDecisions(c, ["accepted", "accepted", "accepted"])).toBe(
      "Keep. Keep more.",
    );
  });

  it("throws on mismatched decision count", () => {
    const c = chunks("A.", "B.");
    expect(() => applyDecisions(c, [])).toThrow(/got 0 decisions for 1 chunks/);
  });
});
