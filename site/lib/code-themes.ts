// Shiki themes in Chain's palette. Plain JSON-shaped objects on purpose:
// next.config passes them to rehype-pretty-code, and Turbopack only accepts
// serializable plugin options.

function codeTheme(
  name: string,
  type: "dark" | "light",
  c: { bg: string; fg: string; keyword: string; string: string; comment: string; constant: string; type: string; fn: string }
) {
  return {
    name,
    type,
    colors: { "editor.background": c.bg, "editor.foreground": c.fg },
    tokenColors: [
      { scope: ["comment", "punctuation.definition.comment"], settings: { foreground: c.comment, fontStyle: "italic" } },
      {
        scope: ["keyword", "storage", "storage.type", "storage.modifier", "keyword.operator.new", "keyword.control"],
        settings: { foreground: c.keyword }
      },
      { scope: ["string", "string.quoted", "string.template"], settings: { foreground: c.string } },
      { scope: ["constant.numeric", "constant.language", "support.constant"], settings: { foreground: c.constant } },
      {
        scope: ["entity.name.type", "entity.name.class", "support.type", "entity.other.inherited-class"],
        settings: { foreground: c.type }
      },
      { scope: ["entity.name.function", "support.function", "meta.function-call"], settings: { foreground: c.fn } },
      { scope: ["variable.parameter"], settings: { foreground: c.fg, fontStyle: "italic" } },
      { scope: ["punctuation", "meta.brace"], settings: { foreground: c.comment } }
    ]
  };
}

export const chainDark = codeTheme("chain-dark", "dark", {
  bg: "#1D222C",
  fg: "#EEF2E4",
  keyword: "#C5F74F",
  string: "#D9C98F",
  comment: "#7E8577",
  constant: "#8FD3C4",
  type: "#B9C4A3",
  fn: "#F5F8EE"
});

export const chainLight = codeTheme("chain-light", "light", {
  bg: "#E3E8D6",
  fg: "#171B24",
  keyword: "#3F5C00",
  string: "#7A5A12",
  comment: "#6A7163",
  constant: "#1B6B5E",
  type: "#4A5540",
  fn: "#171B24"
});
