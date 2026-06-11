/**
 * Flesch-Kincaid grade level for the editor's readability indicator.
 *
 * FK grade = 0.39 * (words/sentences) + 11.8 * (syllables/words) - 15.59
 *
 * Syllables are estimated with the standard vowel-group heuristic
 * (consecutive vowels = one group, silent trailing "e" dropped, every
 * word has at least one). That's accurate enough for a trend indicator —
 * the chip is a nudge, not a gate, and invented fantasy names only move
 * the average slightly on real prose.
 */

const VOWELS = /[aeiouy]/;

export function countSyllables(word: string): number {
  const w = word.toLowerCase().replace(/[^a-z]/g, "");
  if (w.length === 0) return 0;
  if (w.length <= 3) return 1;

  let count = 0;
  let prevVowel = false;
  for (const ch of w) {
    const isVowel = VOWELS.test(ch);
    if (isVowel && !prevVowel) count += 1;
    prevVowel = isVowel;
  }
  // Silent trailing e ("table", "grave") — but not "le" endings ("table"
  // keeps its second syllable via the l+e rule below).
  if (w.endsWith("e") && !w.endsWith("le") && count > 1) count -= 1;
  return Math.max(1, count);
}

export interface ReadabilityScore {
  grade: number;
  words: number;
  sentences: number;
}

/** Minimum words before a score is meaningful. */
const MIN_WORDS = 30;

export function fleschKincaidGrade(text: string): ReadabilityScore | null {
  // Strip the light Markdown the editor produces so syntax doesn't skew
  // word/sentence boundaries.
  const plain = text
    .replace(/[#*_>`~[\]()]/g, " ")
    .replace(/\s+/g, " ")
    .trim();
  if (!plain) return null;

  const words = plain.split(" ").filter((w) => /[a-zA-Z]/.test(w));
  if (words.length < MIN_WORDS) return null;

  // Sentence terminators; em-dash interruptions and ellipses don't end
  // sentences. Guard against zero.
  const sentences = Math.max(1, (plain.match(/[.!?]+(\s|$)/g) ?? []).length);

  const syllables = words.reduce((acc, w) => acc + countSyllables(w), 0);
  const grade =
    0.39 * (words.length / sentences) + 11.8 * (syllables / words.length) - 15.59;

  return {
    grade: Math.max(0, Math.round(grade * 10) / 10),
    words: words.length,
    sentences,
  };
}
