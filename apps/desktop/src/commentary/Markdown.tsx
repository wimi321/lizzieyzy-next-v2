import { type Inline, parseMarkdown } from "./blocks";

function Inlines({ inlines }: { inlines: readonly Inline[] }) {
  return (
    <>
      {inlines.map((inline, index) => {
        switch (inline.kind) {
          case "strong":
            return <strong key={index}>{inline.text}</strong>;
          case "em":
            return <em key={index}>{inline.text}</em>;
          case "code":
            return <code key={index}>{inline.text}</code>;
          default:
            return inline.text;
        }
      })}
    </>
  );
}

/** An AI reply as React text: no HTML, links or images reach the page. */
export function Markdown({ text, className = "markdown" }: { text: string; className?: string }) {
  return (
    <div className={className}>
      {parseMarkdown(text).map((block, index) => {
        switch (block.kind) {
          case "heading": {
            const Tag = (["h3", "h4", "h5"] as const)[block.level - 1] ?? "h5";
            return (
              <Tag key={index}>
                <Inlines inlines={block.inlines} />
              </Tag>
            );
          }
          case "list": {
            const Tag = block.ordered ? "ol" : "ul";
            return (
              <Tag key={index}>
                {block.items.map((item, at) => (
                  <li key={at}>
                    <Inlines inlines={item} />
                  </li>
                ))}
              </Tag>
            );
          }
          case "quote":
            return (
              <blockquote key={index}>
                <Inlines inlines={block.inlines} />
              </blockquote>
            );
          case "code":
            return <pre key={index}>{block.text}</pre>;
          default:
            return (
              <p key={index}>
                <Inlines inlines={block.inlines} />
              </p>
            );
        }
      })}
    </div>
  );
}
