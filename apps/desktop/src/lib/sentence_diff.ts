/**
 * Sentence-level diff for the track-changes review UI.
 *
 * Why sentence-level (not word-level): the voice fingerprint scores text on
 * sentence rhythm and dialogue ratios — features that only make sense at
 * sentence granularity. A reviewer who's evaluating whether the AI matched
 * their voice wants to see "did THIS sentence sound like me?", not "the
 * model swapped 'big' for 'massive'." The user picked this mode explicitly
 * in the Phase 6.3 kickoff.
 *
 * Algorithm: a simple LCS (longest common subsequence) on the sentence
 * arrays, then a left-to-right walk that emits Equal / Delete / Insert
 * chunks. Adjacent Delete + Insert pairs are coalesced into a single
 * Replace chunk so the UI can render them as one decision unit.
 *
 * Complexity: O(n*m) time and space, n = original sentences, m = candidate
 * sentences. For a scene-sized comparison (~50 sentences each), that's
 * 2500 cells — instant on a modern laptop.
 */

/** Granular split of prose into sentences, preserving trailing whitespace. */
export interface Sentence {
  text: string;
  /** Whitespace (including newlines) that follows this sentence. */
  trailing: string;
}

export type DiffOp = "equal" | "delete" | "insert" | "replace";

export interface DiffChunk {
  op: DiffOp;
  /** Sentences from the original text (present for `equal`, `delete`, `replace`). */
  original: Sentence[];
  /** Sentences from the candidate text (present for `equal`, `insert`, `replace`). */
  candidate: Sentence[];
}

/**
 * Split a string into sentences for diffing.
 *
 * Boundaries: `. ! ?` followed by whitespace or end-of-string, plus
 * blank-line breaks (so paragraph boundaries align with sentence
 * boundaries). Common abbreviations (Mr., Mrs., Dr., e.g., i.e., etc.)
 * suppress the split. Quoted dialogue ends are respected — a closing
 * quote after terminator counts as part of the sentence.
 *
 * The split preserves trailing whitespace so we can recompose the original
 * string verbatim by joining `sentence.text + sentence.trailing`.
 */
export function splitSentences(input: string): Sentence[] {
  if (input.length === 0) return [];
  const ABBREV = new Set([
    "mr",
    "mrs",
    "ms",
    "dr",
    "st",
    "jr",
    "sr",
    "vs",
    "etc",
    "e.g",
    "i.e",
    "cf",
    "no",
    "rev",
    "prof",
    "gen",
    "hon",
    "lt",
    "sgt",
    "fr",
    "ft",
  ]);
  const out: Sentence[] = [];
  let buf = "";
  let i = 0;
  while (i < input.length) {
    const ch = input[i] ?? "";
    buf += ch;
    const isTerminator = ch === "." || ch === "!" || ch === "?";
    if (isTerminator) {
      // Consume any trailing quotes/parens that are part of the sentence
      // (e.g. dialogue: She said, "Hi.")
      while (i + 1 < input.length && /["'”’)\]}]/.test(input[i + 1] ?? "")) {
        i += 1;
        buf += input[i] ?? "";
      }
      // Look at the next non-space character to decide if this is really
      // an end-of-sentence. If there's no following text, it always is.
      let lookahead = i + 1;
      while (lookahead < input.length && input[lookahead] === " ") {
        lookahead += 1;
      }
      const next = input[lookahead];
      const isAtEnd = lookahead >= input.length;
      const followedByCapitalOrParaBreak =
        next === undefined ||
        next === "\n" ||
        (next !== undefined && next >= "A" && next <= "Z") ||
        next === '"' ||
        next === "'" ||
        next === "“";
      if (!isAtEnd && !followedByCapitalOrParaBreak) {
        i += 1;
        continue;
      }
      // Suppress on common abbreviations: look back over the last word.
      const lastWordMatch = buf.match(/([A-Za-z.]+)\.\s*$/);
      if (lastWordMatch) {
        const word = (lastWordMatch[1] ?? "").toLowerCase().replace(/\.+$/, "");
        if (ABBREV.has(word)) {
          i += 1;
          continue;
        }
      }
      // Eat trailing whitespace as part of this sentence's trailing field.
      let trailing = "";
      let j = i + 1;
      while (j < input.length && (input[j] === " " || input[j] === "\t")) {
        trailing += input[j] ?? "";
        j += 1;
      }
      // Newlines: take up to and including the first run of blank lines, so
      // paragraph breaks anchor to whichever sentence ended the paragraph.
      while (j < input.length && input[j] === "\n") {
        trailing += input[j] ?? "";
        j += 1;
      }
      out.push({ text: buf, trailing });
      buf = "";
      i = j;
      continue;
    }
    // Paragraph break without terminator (e.g. fragment ending in mid-air).
    if (ch === "\n" && i + 1 < input.length && input[i + 1] === "\n") {
      // Eat the run of blank lines into trailing.
      let trailing = "";
      let j = i + 1;
      while (j < input.length && input[j] === "\n") {
        trailing += input[j] ?? "";
        j += 1;
      }
      // The newline we already consumed is part of `buf`; strip and add to trailing.
      const stripped = buf.slice(0, -1);
      trailing = "\n" + trailing;
      out.push({ text: stripped, trailing });
      buf = "";
      i = j;
      continue;
    }
    i += 1;
  }
  if (buf.length > 0) {
    out.push({ text: buf, trailing: "" });
  }
  return out;
}

/**
 * Inverse of splitSentences. Given an array of sentences, reconstructs the
 * full string by joining text + trailing. For sentences that did not have
 * a natural trailer (final sentence without newline), nothing is added.
 */
export function joinSentences(sentences: Sentence[]): string {
  return sentences.map((s) => s.text + s.trailing).join("");
}

/**
 * Compute a sentence-level diff between two prose strings.
 *
 * Returns an array of DiffChunks. Adjacent (Delete, Insert) pairs are
 * coalesced into a single Replace chunk for cleaner UI rendering.
 */
export function diffSentences(original: string, candidate: string): DiffChunk[] {
  const a = splitSentences(original);
  const b = splitSentences(candidate);
  const raw = lcsDiff(a, b);
  return coalesce(raw);
}

/** Raw three-op diff (no Replace coalescing yet). */
type RawOp = { op: "equal" | "delete" | "insert"; a?: Sentence; b?: Sentence };

function lcsDiff(a: Sentence[], b: Sentence[]): RawOp[] {
  const n = a.length;
  const m = b.length;
  if (n === 0 && m === 0) return [];
  if (n === 0) return b.map((s) => ({ op: "insert", b: s }));
  if (m === 0) return a.map((s) => ({ op: "delete", a: s }));

  // Sentence-equality predicate — exact text match only. Whitespace
  // differences count as differences (intentional: the writer cares).
  const eq = (x: Sentence, y: Sentence): boolean => x.text === y.text;

  // LCS table.
  const dp: number[][] = Array.from({ length: n + 1 }, () =>
    new Array<number>(m + 1).fill(0),
  );
  for (let i = 1; i <= n; i += 1) {
    for (let j = 1; j <= m; j += 1) {
      const ai = a[i - 1];
      const bj = b[j - 1];
      if (ai !== undefined && bj !== undefined && eq(ai, bj)) {
        dp[i]![j] = (dp[i - 1]![j - 1] ?? 0) + 1;
      } else {
        dp[i]![j] = Math.max(dp[i - 1]![j] ?? 0, dp[i]![j - 1] ?? 0);
      }
    }
  }

  // Backtrack.
  const ops: RawOp[] = [];
  let i = n;
  let j = m;
  while (i > 0 && j > 0) {
    const ai = a[i - 1];
    const bj = b[j - 1];
    if (ai !== undefined && bj !== undefined && eq(ai, bj)) {
      ops.push({ op: "equal", a: ai, b: bj });
      i -= 1;
      j -= 1;
    } else if ((dp[i - 1]![j] ?? 0) >= (dp[i]![j - 1] ?? 0)) {
      ops.push({ op: "delete", a: ai });
      i -= 1;
    } else {
      ops.push({ op: "insert", b: bj });
      j -= 1;
    }
  }
  while (i > 0) {
    ops.push({ op: "delete", a: a[i - 1] });
    i -= 1;
  }
  while (j > 0) {
    ops.push({ op: "insert", b: b[j - 1] });
    j -= 1;
  }
  ops.reverse();
  return ops;
}

/**
 * Coalesce runs of (delete..., insert...) into Replace chunks. Pure
 * Equal/Delete/Insert runs become their own chunks.
 */
function coalesce(ops: RawOp[]): DiffChunk[] {
  const out: DiffChunk[] = [];
  let i = 0;
  while (i < ops.length) {
    const op = ops[i];
    if (!op) {
      i += 1;
      continue;
    }
    if (op.op === "equal") {
      // Gather a run of equals.
      const orig: Sentence[] = [];
      const cand: Sentence[] = [];
      while (i < ops.length && ops[i]?.op === "equal") {
        const cur = ops[i]!;
        if (cur.a) orig.push(cur.a);
        if (cur.b) cand.push(cur.b);
        i += 1;
      }
      out.push({ op: "equal", original: orig, candidate: cand });
      continue;
    }

    // Non-equal: collect contiguous deletes and inserts and coalesce.
    const deletes: Sentence[] = [];
    const inserts: Sentence[] = [];
    while (i < ops.length && ops[i] && ops[i]!.op !== "equal") {
      const cur = ops[i]!;
      if (cur.op === "delete" && cur.a) deletes.push(cur.a);
      else if (cur.op === "insert" && cur.b) inserts.push(cur.b);
      i += 1;
    }
    if (deletes.length > 0 && inserts.length > 0) {
      out.push({ op: "replace", original: deletes, candidate: inserts });
    } else if (deletes.length > 0) {
      out.push({ op: "delete", original: deletes, candidate: [] });
    } else if (inserts.length > 0) {
      out.push({ op: "insert", original: [], candidate: inserts });
    }
  }
  return out;
}

/**
 * Apply a set of accept/reject decisions to produce the final text.
 *
 * For each chunk:
 *   - `equal` chunks pass through (original == candidate).
 *   - `delete` chunks: if accepted, the original sentences are dropped;
 *     if rejected, they're kept.
 *   - `insert` chunks: if accepted, the candidate sentences appear here;
 *     if rejected, nothing is inserted.
 *   - `replace` chunks: if accepted, candidate replaces original; if
 *     rejected, original stands.
 */
export type Decision = "accepted" | "rejected";

export function applyDecisions(chunks: DiffChunk[], decisions: Decision[]): string {
  if (decisions.length !== chunks.length) {
    throw new Error(
      `applyDecisions: got ${decisions.length} decisions for ${chunks.length} chunks`,
    );
  }

  // An equal chunk's two sides have identical sentence TEXT but can differ
  // in trailing whitespace (because the original and candidate may have
  // had different spacing around it). At the boundary between an equal
  // chunk and a following accepted change, we need to use the trailing
  // that matches what's coming next — otherwise we end up concatenating
  // sentences with no separator (e.g. "Bookends.New middle.").
  const sourceSide = (i: number): "original" | "candidate" | "none" => {
    const c = chunks[i];
    const d = decisions[i];
    if (!c) return "none";
    switch (c.op) {
      case "equal":
        return "original";
      case "delete":
        return d === "rejected" ? "original" : "none";
      case "insert":
        return d === "accepted" ? "candidate" : "none";
      case "replace":
        return d === "accepted" ? "candidate" : "original";
    }
  };

  const out: Sentence[] = [];
  chunks.forEach((c, idx) => {
    const d = decisions[idx];
    switch (c.op) {
      case "equal": {
        if (c.original.length === 0) break;
        // All but the last: their trailing is internal to the equal run.
        out.push(...c.original.slice(0, -1));
        // The last sentence's trailing must match whichever side the next
        // chunk's content comes from.
        let nextSource: "original" | "candidate" | "none" = "none";
        for (let j = idx + 1; j < chunks.length; j += 1) {
          const s = sourceSide(j);
          if (s !== "none") {
            nextSource = s;
            break;
          }
        }
        const lastOriginal = c.original[c.original.length - 1]!;
        const lastCandidate = c.candidate[c.candidate.length - 1] ?? lastOriginal;
        out.push(nextSource === "candidate" ? lastCandidate : lastOriginal);
        break;
      }
      case "delete":
        if (d === "rejected") out.push(...c.original);
        break;
      case "insert":
        if (d === "accepted") out.push(...c.candidate);
        break;
      case "replace":
        out.push(...(d === "accepted" ? c.candidate : c.original));
        break;
    }
  });
  return joinSentences(out);
}
