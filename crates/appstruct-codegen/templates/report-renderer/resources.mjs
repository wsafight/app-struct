import { parse } from "parse5";
import { parse as parseCss, walk as walkCss } from "css-tree";

const blocked = () => { throw new Error("REPORT_BLOCKED_RESOURCE"); };
function url(value) {
  if (value === "" || value.startsWith("#")) return;
  if (/^data:(image\/(png|jpeg|webp)|font\/(woff2?|otf|ttf));base64,[A-Za-z0-9+/=]+$/.test(value)) return;
  blocked();
}
function css(value, context = "stylesheet") {
  let ast;
  try { ast = parseCss(value, { context }); }
  catch { blocked(); }
  walkCss(ast, (node) => {
    if (node.type === "Url") url(node.value);
    if (node.type === "Atrule" && node.name.toLowerCase() === "import") blocked();
    if (node.type === "Raw") blocked();
  });
}
export function validateResources(html) {
  const visit = (node) => {
    if (["script", "iframe", "frame", "object", "embed", "base", "form", "svg", "math", "template"].includes(node.tagName)) blocked();
    for (const { name, value } of node.attrs ?? []) {
      if (name.startsWith("on") || ["srcset", "srcdoc", "background", "ping"].includes(name)) blocked();
      if (node.tagName === "meta" && name === "http-equiv") blocked();
      if (["src", "href", "poster", "action"].includes(name)) url(value);
      if (name === "style") css(value, "declarationList");
    }
    if (node.tagName === "style") css(node.childNodes.map((child) => child.value ?? "").join(""));
    for (const child of node.childNodes ?? []) visit(child);
  };
  visit(parse(html));
}
