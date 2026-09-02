import type { ReactNode } from "react";

type MarkdownBlock =
  | { kind: "paragraph"; lines: string[] }
  | { kind: "ordered"; items: Array<{ value: number; text: string }> }
  | { kind: "unordered"; items: string[] }
  | { kind: "heading"; level: number; text: string }
  | { kind: "code"; text: string };

export function AgentMarkdown({ content }: { content: string }) {
  return <div className="agent-markdown">
    {parseMarkdown(content).map((block, index) => renderBlock(block, index))}
  </div>;
}

function parseMarkdown(content: string): MarkdownBlock[] {
  const blocks: MarkdownBlock[] = [];
  const lines = content.replace(/\r\n?/g, "\n").split("\n");
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];
    if (!line.trim()) {
      index += 1;
      continue;
    }
    if (line.startsWith("```")) {
      const codeLines: string[] = [];
      index += 1;
      while (index < lines.length && !lines[index].startsWith("```")) {
        codeLines.push(lines[index]);
        index += 1;
      }
      if (index < lines.length) index += 1;
      blocks.push({ kind: "code", text: codeLines.join("\n") });
      continue;
    }
    const heading = /^(#{1,3})\s+(.+)$/.exec(line);
    if (heading) {
      blocks.push({ kind: "heading", level: heading[1].length, text: heading[2] });
      index += 1;
      continue;
    }
    const ordered = /^(\d+)\.\s+(.+)$/.exec(line);
    if (ordered) {
      const items: Array<{ value: number; text: string }> = [];
      while (index < lines.length) {
        const item = /^(\d+)\.\s+(.+)$/.exec(lines[index]);
        if (!item) break;
        items.push({ value: Number(item[1]), text: item[2] });
        index += 1;
      }
      blocks.push({ kind: "ordered", items });
      continue;
    }
    const unordered = /^[-*+]\s+(.+)$/.exec(line);
    if (unordered) {
      const items: string[] = [];
      while (index < lines.length) {
        const item = /^[-*+]\s+(.+)$/.exec(lines[index]);
        if (!item) break;
        items.push(item[1]);
        index += 1;
      }
      blocks.push({ kind: "unordered", items });
      continue;
    }
    const paragraph: string[] = [];
    while (index < lines.length && lines[index].trim()) {
      if (lines[index].startsWith("```") || /^(#{1,3})\s+/.test(lines[index]) || /^(\d+)\.\s+/.test(lines[index]) || /^[-*+]\s+/.test(lines[index])) break;
      paragraph.push(lines[index]);
      index += 1;
    }
    if (paragraph.length) blocks.push({ kind: "paragraph", lines: paragraph });
  }
  return blocks;
}

function renderBlock(block: MarkdownBlock, key: number) {
  switch (block.kind) {
    case "heading":
      if (block.level === 1) return <h1 key={key}>{renderInline(block.text)}</h1>;
      if (block.level === 2) return <h2 key={key}>{renderInline(block.text)}</h2>;
      return <h3 key={key}>{renderInline(block.text)}</h3>;
    case "ordered":
      return <ol key={key} start={block.items[0]?.value}>{block.items.map((item, index) => <li key={index}>{renderInline(item.text)}</li>)}</ol>;
    case "unordered":
      return <ul key={key}>{block.items.map((item, index) => <li key={index}>{renderInline(item)}</li>)}</ul>;
    case "code":
      return <pre key={key}><code>{block.text}</code></pre>;
    case "paragraph":
      return <p key={key}>{block.lines.map((line, index) => <span key={index}>{index > 0 && <br />}{renderInline(line)}</span>)}</p>;
  }
}

function renderInline(value: string): ReactNode[] {
  const nodes: ReactNode[] = [];
  let cursor = 0;
  while (cursor < value.length) {
    const boldAt = value.indexOf("**", cursor);
    const codeAt = value.indexOf("`", cursor);
    const markerAt = [boldAt, codeAt].filter((position) => position >= 0).sort((left, right) => left - right)[0];
    if (markerAt === undefined) {
      nodes.push(value.slice(cursor));
      break;
    }
    if (markerAt > cursor) nodes.push(value.slice(cursor, markerAt));
    if (markerAt === boldAt) {
      const end = value.indexOf("**", markerAt + 2);
      if (end >= 0) {
        nodes.push(<strong key={markerAt}>{renderInline(value.slice(markerAt + 2, end))}</strong>);
        cursor = end + 2;
        continue;
      }
    } else {
      const end = value.indexOf("`", markerAt + 1);
      if (end >= 0) {
        nodes.push(<code key={markerAt}>{value.slice(markerAt + 1, end)}</code>);
        cursor = end + 1;
        continue;
      }
    }
    nodes.push(value[markerAt]);
    cursor = markerAt + 1;
  }
  return nodes;
}
