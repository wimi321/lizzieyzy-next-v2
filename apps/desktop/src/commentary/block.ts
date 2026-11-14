/**
 * The AI commentary block Rust writes into a node's comment (`crates/commentary/src/codec.rs`),
 * split out for display only: the comment panel shows it under 「AI 解说」, apart from the
 * user's own text. Writing and replacing blocks stays in Rust.
 */
const BEGIN = "[LizzieYzy AI Commentary BEGIN]";
const END = "[LizzieYzy AI Commentary END]";
/** Rust always marks a follow-up this way, whatever the interface language. */
const FOLLOW_UP = /\n\n追问：([^\n]*)\n\n/;

export interface CommentarySection {
  /** Null for the first answer; the question for each follow-up. */
  question: string | null;
  body: string;
}

export interface CommentaryBlock {
  model: string;
  generatedAt: string;
  sections: CommentarySection[];
}

export interface SplitComment {
  /** The user's own comment, with every complete AI block taken out. */
  own: string;
  /** The last complete AI block, as Rust's reader takes it. */
  ai: CommentaryBlock | null;
}

/** Complete blocks as [start, end) ranges; an unfinished block stays the user's text. */
function ranges(text: string): [number, number][] {
  const found: [number, number][] = [];
  let from = text.length;
  while (from > 0) {
    const start = text.lastIndexOf(BEGIN, from - 1);
    if (start < 0) break;
    const end = text.indexOf(END, start + BEGIN.length);
    if (end >= 0 && (found.length === 0 || end + END.length <= found[0]![0])) found.unshift([start, end + END.length]);
    from = start;
  }
  return found;
}

function parse(inner: string): CommentaryBlock {
  const lines = inner.replace(/^\n/, "").split("\n");
  let model = "";
  let generatedAt = "";
  let at = 0;
  for (; at < lines.length && lines[at] !== ""; at += 1) {
    const line = lines[at] ?? "";
    if (line.startsWith("model=")) model = line.slice("model=".length);
    else if (line.startsWith("generatedAt=")) generatedAt = line.slice("generatedAt=".length);
  }
  const body = lines.slice(at + 1).join("\n").trim();
  const parts = body.split(FOLLOW_UP);
  const sections: CommentarySection[] = [{ question: null, body: (parts[0] ?? "").trim() }];
  for (let index = 1; index + 1 < parts.length; index += 2) {
    sections.push({ question: (parts[index] ?? "").trim(), body: (parts[index + 1] ?? "").trim() });
  }
  return { model, generatedAt, sections };
}

export function splitComment(comment: string): SplitComment {
  const text = comment.replace(/\r\n?/g, "\n");
  const blocks = ranges(text);
  const last = blocks[blocks.length - 1];
  if (!last) return { own: comment, ai: null };
  let own = "";
  let from = 0;
  for (const [start, end] of blocks) {
    own += text.slice(from, start);
    from = end;
  }
  own += text.slice(from);
  return { own: own.trim(), ai: parse(text.slice(last[0] + BEGIN.length, last[1] - END.length)) };
}

/** "2026-11-14T09:30:12Z" → "2026-11-14 09:30", as the panel lists it. */
export function shortGeneratedAt(value: string): string {
  const match = /^(\d{4}-\d{2}-\d{2})T(\d{2}:\d{2})/.exec(value);
  return match ? `${match[1]} ${match[2]}` : value;
}
