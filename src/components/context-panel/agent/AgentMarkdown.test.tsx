import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AgentMarkdown } from "./AgentMarkdown";

describe("AgentMarkdown", () => {
  it("renders the analysis markdown subset without injecting HTML", () => {
    const markup = renderToStaticMarkup(<AgentMarkdown content={"磁盘情况良好。\n\n1. **根分区 /** 使用 `12%`\n2. **/boot** 正常\n\n**结论**：无需清理。\n\n<script>unsafe()</script>"} />);
    expect(markup).toContain("<ol");
    expect(markup).toContain("<strong>根分区 /</strong>");
    expect(markup).toContain("<code>12%</code>");
    expect(markup).toContain("&lt;script&gt;unsafe()&lt;/script&gt;");
  });
});
