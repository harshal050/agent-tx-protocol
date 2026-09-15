import { Callout, type CalloutKind } from "@agenttx/ui/primitives";
import { CopyButton } from "@agenttx/ui/interactive";
import type { Root } from "hast";
import { toJsxRuntime, type Components, type Jsx } from "hast-util-to-jsx-runtime";
import Link from "next/link";
import type { ComponentProps } from "react";
import { Fragment, jsx, jsxs } from "react/jsx-runtime";

const CALLOUT_KINDS = new Set<CalloutKind>(["note", "tip", "important", "warning", "caution"]);

const LANGUAGE_LABELS: Record<string, string> = {
  bash: "Terminal",
  sh: "Terminal",
  text: "Text",
  ts: "TypeScript",
  tsx: "TypeScript",
  js: "JavaScript",
  json: "JSON",
  rust: "Rust",
  python: "Python",
  yaml: "YAML",
  toml: "TOML",
  proto: "Protobuf",
};

type DataProps = Record<string, unknown>;

export function CodeBlock(props: ComponentProps<"pre">) {
  const data = props as DataProps;
  const language = typeof data["data-language"] === "string" ? data["data-language"] : "text";
  const code = typeof data["data-code"] === "string" ? data["data-code"] : "";
  const { className, style, children } = props;
  return (
    <div className="code-block group">
      <div className="code-block-header">
        <span>{LANGUAGE_LABELS[language] ?? language}</span>
        <CopyButton value={code} className="size-7 border-transparent bg-transparent" />
      </div>
      <pre className={className} style={style} data-language={language}>
        {children}
      </pre>
    </div>
  );
}

function Anchor({ href = "", children, ...rest }: ComponentProps<"a">) {
  if (href.startsWith("/")) {
    return (
      <Link href={href} className={rest.className}>
        {children}
      </Link>
    );
  }
  return (
    <a href={href} {...rest}>
      {children}
    </a>
  );
}

function Aside(props: ComponentProps<"aside">) {
  const kind = (props as DataProps)["data-callout"];
  if (typeof kind === "string" && CALLOUT_KINDS.has(kind as CalloutKind)) {
    return <Callout kind={kind as CalloutKind}>{props.children}</Callout>;
  }
  return <aside className={props.className}>{props.children}</aside>;
}

function Table(props: ComponentProps<"table">) {
  return (
    <div className="table-wrap">
      <table {...props} />
    </div>
  );
}

function Image({ src, alt = "", ...rest }: ComponentProps<"img">) {
  // Remote images from GitHub have unknown dimensions; next/image is not a fit here.
  // eslint-disable-next-line @next/next/no-img-element
  return <img src={typeof src === "string" ? src : undefined} alt={alt} loading="lazy" decoding="async" {...rest} />;
}

const components = {
  a: Anchor,
  aside: Aside,
  img: Image,
  pre: CodeBlock,
  table: Table,
} as Partial<Components>;

/** Renders a hast tree produced by `@agenttx/content` as React server components. */
export function Markdown({ tree }: { tree: Root }) {
  return toJsxRuntime(tree, {
    Fragment,
    jsx: jsx as Jsx,
    jsxs: jsxs as Jsx,
    components,
    ignoreInvalidStyle: true,
  });
}
