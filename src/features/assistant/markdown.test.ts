// ยกมาจาก PuriCat src/features/agent/markdown.test.ts (ตัดเทสต์ของ SQL ออก)
import { describe, expect, it } from "vitest";

import { parseInline, parseMarkdown } from "./markdown";

describe("parseMarkdown", () => {
  it("splits an answer into paragraphs, code, headings and lists", () => {
    const blocks = parseMarkdown(
      [
        "## นับคำสั่งซื้อ",
        "ใช้คำสั่งนี้ กับตาราง `orders`:",
        "",
        "```sql",
        "SELECT COUNT(*)",
        "FROM orders;",
        "```",
        "",
        "- **ช้า** ถ้าไม่มี index",
        "- อ่านอย่างเดียว",
        "  ไม่แก้ข้อมูล",
        "",
        "1. รัน",
        "2) ตรวจผล",
      ].join("\n"),
    );
    expect(blocks).toEqual([
      { kind: "heading", level: 2, inline: [{ kind: "text", text: "นับคำสั่งซื้อ" }] },
      {
        kind: "paragraph",
        inline: [
          { kind: "text", text: "ใช้คำสั่งนี้ กับตาราง " },
          { kind: "code", text: "orders" },
          { kind: "text", text: ":" },
        ],
      },
      { kind: "code", lang: "sql", text: "SELECT COUNT(*)\nFROM orders;", open: false },
      {
        kind: "list",
        ordered: false,
        items: [
          [
            { kind: "strong", text: "ช้า" },
            { kind: "text", text: " ถ้าไม่มี index" },
          ],
          [{ kind: "text", text: "อ่านอย่างเดียว\nไม่แก้ข้อมูล" }],
        ],
      },
      {
        kind: "list",
        ordered: true,
        items: [[{ kind: "text", text: "รัน" }], [{ kind: "text", text: "ตรวจผล" }]],
      },
    ]);
  });

  it("keeps a code block that is still streaming, and everything inside a fence verbatim", () => {
    expect(parseMarkdown("Here:\n```sql\nSELECT 1")).toEqual([
      { kind: "paragraph", inline: [{ kind: "text", text: "Here:" }] },
      { kind: "code", lang: "sql", text: "SELECT 1", open: true },
    ]);
    const [block] = parseMarkdown("```\n# not a heading\n- not a list\n**x** `y`\n\n| a |\n```");
    expect(block).toEqual({ kind: "code", lang: "", text: "# not a heading\n- not a list\n**x** `y`\n\n| a |", open: false });
    // A fence with a language inside a block does not close it.
    expect(parseMarkdown("```md\n```sql\nx\n```")).toEqual([{ kind: "code", lang: "md", text: "```sql\nx", open: false }]);
  });

  it("reads simple tables", () => {
    const [table, after] = parseMarkdown("| col | type |\n|---|:--|\n| id | `int` |\n| name | text |\n\nDone");
    expect(table).toEqual({
      kind: "table",
      header: [[{ kind: "text", text: "col" }], [{ kind: "text", text: "type" }]],
      rows: [
        [[{ kind: "text", text: "id" }], [{ kind: "code", text: "int" }]],
        [[{ kind: "text", text: "name" }], [{ kind: "text", text: "text" }]],
      ],
    });
    expect(after).toEqual({ kind: "paragraph", inline: [{ kind: "text", text: "Done" }] });
    // A pipe in prose is not a table.
    expect(parseMarkdown("a | b\nc")[0]?.kind).toBe("paragraph");
  });

  it("never treats markup-looking text as anything but text", () => {
    expect(parseInline('<img src=x onerror="alert(1)"> **b** *not em* a*b')).toEqual([
      { kind: "text", text: '<img src=x onerror="alert(1)"> ' },
      { kind: "strong", text: "b" },
      { kind: "text", text: " *not em* a*b" },
    ]);
    expect(parseMarkdown("")).toEqual([]);
  });
});
