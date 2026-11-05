import { workspacePath } from "../generated/registry";

export function safeReturnTo(value: unknown): string {
  if (
    typeof value !== "string" ||
    !value.startsWith("/") ||
    value.startsWith("//") ||
    value.includes("\\") ||
    [...value].some((character) => {
      const code = character.charCodeAt(0);
      return code < 32 || code === 127;
    })
  )
    return workspacePath;
  try {
    const target = new URL(value, "https://appstruct.local");
    if (
      target.origin !== "https://appstruct.local" ||
      /^\/(login|register|forgot-password|reset-password|verify-email|api)(\/|$)/.test(
        target.pathname,
      )
    )
      return workspacePath;
    return `${target.pathname}${target.search}${target.hash}`;
  } catch {
    return workspacePath;
  }
}

export function returnTo(location: {
  searchStr?: string;
  state?: unknown;
}): string {
  const next = new URLSearchParams(location.searchStr ?? "").get("next");
  if (next) return safeReturnTo(next);
  const from = (
    location.state as {
      from?: { pathname?: string; searchStr?: string; hash?: string };
    } | null
  )?.from;
  const search = from?.searchStr ? `?${from.searchStr.replace(/^\?/, "")}` : "";
  const hash = from?.hash ? `#${from.hash.replace(/^#/, "")}` : "";
  return safeReturnTo(
    from?.pathname ? `${from.pathname}${search}${hash}` : workspacePath,
  );
}
