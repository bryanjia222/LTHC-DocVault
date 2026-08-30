import DOMPurify from "dompurify";
import type { QinbixinMessage } from "../../composables/useQinbixin";

export type SanitizedQinbixinMessage = QinbixinMessage & {
  safeContent: string;
};

export const QINBIXIN_SANITIZE_CONFIG = {
  ALLOWED_TAGS: [
    "p",
    "br",
    "strong",
    "em",
    "b",
    "i",
    "u",
    "s",
    "strike",
    "span",
    "a",
    "img",
    "video",
    "audio",
    "source",
    "ol",
    "ul",
    "li",
    "blockquote",
    "pre",
    "code",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
    "table",
    "thead",
    "tbody",
    "tr",
    "td",
    "th",
    "sub",
    "sup",
    "figure",
    "figcaption",
    "caption",
    "colgroup",
    "col",
    "tfoot",
  ],
  ALLOWED_ATTR: [
    "href",
    "target",
    "rel",
    "src",
    "alt",
    "type",
    "controls",
    "preload",
    "width",
    "height",
    "style",
    "class",
    "colspan",
    "rowspan",
    "id",
    "name",
    "start",
    "dir",
    "lang",
    "span",
    "poster",
    "srcset",
    "sizes",
    "datetime",
    "cite",
    "data-pagebreak",
  ],
  ALLOW_DATA_ATTR: false,
  ALLOWED_URI_REGEXP: /^(?:https?:|mailto:|tel:|data:image\/|\/|#)/i,
};

export function sanitizeQinbixinMessage(
  message: QinbixinMessage,
): SanitizedQinbixinMessage {
  return {
    ...message,
    safeContent: DOMPurify.sanitize(message.content, QINBIXIN_SANITIZE_CONFIG),
  };
}

export function sanitizeQinbixinRichContent(html: string): string {
  const content = DOMPurify.sanitize(html, QINBIXIN_SANITIZE_CONFIG).trim();
  const emptyHtml = /^(?:<p>(?:\s|&nbsp;|<br\s*\/?>)*<\/p>|<br\s*\/?>)+$/i.test(
    content,
  );
  return emptyHtml ? "" : content;
}
