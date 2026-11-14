/**
 * The small Markdown subset AI services write: headings, paragraphs, lists, quotes, code, bold,
 * italics and inline code. It produces plain data that React renders as text, so nothing in a
 * reply can become HTML, a link or a script.
 */
export type Inline = { kind: "text" | "strong" | "em" | "code"; text: string };

export type Block =
  | { kind: "heading"; level: 1 | 2 | 3; inlines: Inline[] }
  | { kind: "paragraph"; inlines: Inline[] }
  | { kind: "list"; ordered: boolean; items: Inline[][] }
  | { kind: "quote"; inlines: Inline[] }
  | { kind: "code"; text: string };

const INLINE = /\*\*(.+?)\*\*|__(.+?)__|`([^`\n]+)`|\*(?![\s*])(.+?)(?<![\s*])\*/g;

export function parseInline(text: string): Inline[] {
  const out: Inline[] = [];
  let last = 0;
  for (const match of text.matchAll(INLINE)) {
    const at = match.index ?? 0;
    if (at > last) out.push({ kind: "text", text: text.slice(last, at) });
    const [, strong, under, code, em] = match;
    if (strong !== undefined || under !== undefined) out.push({ kind: "strong", text: strong ?? under ?? "" });
    else if (code !== undefined) out.push({ kind: "code", text: code });
    else out.push({ kind: "em", text: em ?? "" });
    last = at + match[0].length;
  }
  if (last < text.length) out.push({ kind: "text", text: text.slice(last) });
  return out;
}

export function parseMarkdown(source: string): Block[] {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const blocks: Block[] = [];
  let paragraph: string[] = [];
  let quote: string[] = [];
  const flush = () => {
    if (paragraph.length > 0) blocks.push({ kind: "paragraph", inlines: parseInline(paragraph.join("\n")) });
    if (quote.length > 0) blocks.push({ kind: "quote", inlines: parseInline(quote.join("\n")) });
    paragraph = [];
    quote = [];
  };
  for (let index = 0; index < lines.length; index += 1) {
    const line = (lines[index] ?? "").trim();
    if (line.startsWith("```")) {
      flush();
      const code: string[] = [];
      index += 1;
      // An unfinished fence (the reply is still arriving) runs to the end.
      while (index < lines.length && !(lines[index] ?? "").trim().startsWith("```")) {
        code.push(lines[index] ?? "");
        index += 1;
      }
      blocks.push({ kind: "code", text: code.join("\n") });
      continue;
    }
    if (line === "" || /^([-*_])(\s*\1){2,}$/.test(line)) {
      flush();
      continue;
    }
    const heading = /^(#{1,6})\s+(.*?)(\s+#+)?$/.exec(line);
    if (heading) {
      flush();
      const level = Math.min(3, heading[1]?.length ?? 1) as 1 | 2 | 3;
      blocks.push({ kind: "heading", level, inlines: parseInline(heading[2] ?? "") });
      continue;
    }
    const item = /^(?:[-*+]|(\d+)[.)])\s+(.*)$/.exec(line);
    if (item) {
      flush();
      const ordered = item[1] !== undefined;
      const inlines = parseInline(item[2] ?? "");
      const previous = blocks[blocks.length - 1];
      if (previous?.kind === "list" && previous.ordered === ordered) previous.items.push(inlines);
      else blocks.push({ kind: "list", ordered, items: [inlines] });
      continue;
    }
    const quoted = /^>\s?(.*)$/.exec(line);
    if (quoted) {
      if (paragraph.length > 0) flush();
      quote.push(quoted[1] ?? "");
      continue;
    }
    if (quote.length > 0) flush();
    paragraph.push(line);
  }
  flush();
  return blocks;
}
