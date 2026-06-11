import { describe, expect, it } from "vitest";
import { countSyllables, fleschKincaidGrade } from "./readability";

describe("countSyllables", () => {
  it("counts simple words", () => {
    expect(countSyllables("cat")).toBe(1);
    expect(countSyllables("table")).toBe(2);
    expect(countSyllables("dragon")).toBe(2);
    expect(countSyllables("adventure")).toBe(3);
  });

  it("drops silent trailing e", () => {
    expect(countSyllables("grave")).toBe(1);
    expect(countSyllables("blade")).toBe(1);
  });

  it("every word has at least one syllable", () => {
    expect(countSyllables("sky")).toBe(1);
    expect(countSyllables("a")).toBe(1);
  });
});

describe("fleschKincaidGrade", () => {
  it("returns null for short or empty text", () => {
    expect(fleschKincaidGrade("")).toBeNull();
    expect(fleschKincaidGrade("Too short to score.")).toBeNull();
  });

  it("scores simple prose at a low grade", () => {
    const simple =
      "The dog ran fast. The sun was hot. He saw a bird. The bird flew up. " +
      "He sat by the tree. The wind blew soft. It was a good day. He went home. " +
      "Mom made him food. He ate it all and slept.";
    const score = fleschKincaidGrade(simple);
    expect(score).not.toBeNull();
    expect(score!.grade).toBeLessThan(4);
  });

  it("scores complex prose at a higher grade", () => {
    const complex =
      "Notwithstanding the considerable difficulties inherent in establishing " +
      "a comprehensive epistemological framework, contemporary philosophical " +
      "investigations consistently demonstrate that intersubjective verification " +
      "remains indispensable for evaluating ostensibly objective phenomena, " +
      "particularly when methodological assumptions inevitably influence the " +
      "interpretation of empirical observations gathered across heterogeneous contexts.";
    const score = fleschKincaidGrade(complex);
    expect(score).not.toBeNull();
    expect(score!.grade).toBeGreaterThan(12);
  });

  it("ignores markdown syntax", () => {
    const md =
      "# Chapter One\n\nThe dog ran fast. The sun was hot. *He saw a bird.* " +
      "The bird flew up. He sat by the tree. The wind blew soft. It was a good day. " +
      "He went home. Mom made him food. He ate it all and slept.";
    const score = fleschKincaidGrade(md);
    expect(score).not.toBeNull();
    expect(score!.grade).toBeLessThan(4);
  });
});
