import { chinese } from "./ui-messages";
import { Boxes } from "lucide-react";

const defaults = {
  workspace: "Workspace",
  resources: "Resources",
  tools: "Tools",
  apiTokens: "API tokens",
  administration: "Administration",
  auditLog: "Audit log",
  reports: "Reports",
  billing: "Billing",
  organization: "Organization",
  signOut: "Sign out",
  editRecord: "Edit {resource}",
  addRecord: "Add {resource}",
  loadingRecord: "Loading record",
  recordNotFound: "Record not found",
  cancel: "Cancel",
  save: "Save",
  saving: "Saving...",
  retry: "Retry",
  reloadLatest: "Reload latest",
} as const;

export type UIMessage = keyof typeof defaults;
export type ThemeColor =
  | "text"
  | "muted"
  | "surface"
  | "surface-soft"
  | "page"
  | "border"
  | "border-strong"
  | "accent"
  | "accent-hover"
  | "accent-soft"
  | "danger"
  | "danger-hover"
  | "danger-soft";

export interface AppUIOptions {
  locale?: "en" | "zh-CN";
  /** Read-only timestamps use this zone; datetime-local inputs use the browser zone. */
  timeZone?: string;
  translations?: Record<string, string>;
  fieldLabels?: Record<string, Record<string, string>>;
  title?: string;
  logo?: { src: string; alt?: string };
  colorScheme?: "light" | "dark" | "system";
  colors?: Partial<Record<ThemeColor, string>>;
  messages?: Partial<Record<UIMessage, string>>;
}

let options: AppUIOptions = {};
const dateFormatters = new Map<string, Intl.DateTimeFormat>();
const numberFormatters = new Map<string, Intl.NumberFormat>();
let removeThemeListener: (() => void) | undefined;
let appliedColors: ThemeColor[] = [];

/** Configure once in app/web/setup.ts, before the application starts. */
export function configureUI(next: AppUIOptions) {
  // Fail at setup time instead of throwing while rendering a table.
  new Intl.DateTimeFormat(next.locale ?? "en", { timeZone: next.timeZone });
  options = next;
  dateFormatters.clear();
  numberFormatters.clear();
  document.documentElement.lang = uiLocale();
  removeThemeListener?.();
  removeThemeListener = undefined;
  const root = document.documentElement;
  for (const name of appliedColors)
    root.style.removeProperty(`--color-${name}`);
  appliedColors = Object.keys(next.colors ?? {}) as ThemeColor[];
  for (const name of appliedColors) {
    root.style.setProperty(`--color-${name}`, next.colors![name]!);
  }
  if (next.title) document.title = next.title;
  if (next.colorScheme === "system" && window.matchMedia) {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const update = () => {
      root.dataset.theme = media.matches ? "dark" : "light";
    };
    update();
    media.addEventListener("change", update);
    removeThemeListener = () => media.removeEventListener("change", update);
  } else {
    root.dataset.theme = next.colorScheme === "dark" ? "dark" : "light";
  }
}

export function uiText(
  key: UIMessage,
  values: Record<string, string | number> = {},
): string {
  return (options.messages?.[key] ?? t(defaults[key])).replace(
    /\{(\w+)\}/g,
    (token, name: string) =>
      values[name] === undefined ? token : String(values[name]),
  );
}

export function AppBrand({ fallbackTitle }: { fallbackTitle: string }) {
  return (
    <>
      <span className="brand-mark">
        {options.logo ? (
          <img src={options.logo.src} alt={options.logo.alt ?? ""} />
        ) : (
          <Boxes size={21} aria-hidden />
        )}
      </span>
      <span>{options.title ?? fallbackTitle}</span>
    </>
  );
}

/** Resolve standard copy at render/validation time; never translate application data. */
export function t(
  source: string,
  values: Record<string, unknown> = {},
): string {
  return (
    options.translations?.[source] ??
    (uiLocale() === "zh-CN" ? chinese[source] : undefined) ??
    source
  ).replace(/\{(\w+)\}/g, (token, key: string) =>
    values[key] === undefined ? token : String(values[key]),
  );
}

export function uiLocale(): "en" | "zh-CN" {
  return options.locale ?? "en";
}
export function inputTimeZone(): string {
  return Intl.DateTimeFormat().resolvedOptions().timeZone;
}

export function formatDateTime(value: string | Date): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return String(value);
  return dateFormatter(false).format(date);
}

export function formatDate(value: string | Date): string {
  // Calendar dates have no time zone. Never shift a YYYY-MM-DD into another day.
  const calendar =
    typeof value === "string" && /^\d{4}-\d{2}-\d{2}$/.test(value);
  const date = new Date(calendar ? `${value}T00:00:00Z` : value);
  if (Number.isNaN(date.getTime())) return String(value);
  return dateFormatter(calendar, true).format(date);
}

export function formatNumber(
  value: string | number | bigint,
  settings: Intl.NumberFormatOptions = {},
): string {
  const key = JSON.stringify(settings);
  let formatter = numberFormatters.get(key);
  if (!formatter) {
    formatter = new Intl.NumberFormat(uiLocale(), {
      maximumFractionDigits: 28,
      ...settings,
    });
    if (numberFormatters.size >= 32) numberFormatters.clear();
    numberFormatters.set(key, formatter);
  }
  return formatter.format(value as number);
}

function dateFormatter(
  calendar: boolean,
  dateOnly = false,
): Intl.DateTimeFormat {
  const key = `${calendar}:${dateOnly}`;
  let formatter = dateFormatters.get(key);
  if (!formatter) {
    formatter = new Intl.DateTimeFormat(uiLocale(), {
      dateStyle: "medium",
      ...(dateOnly ? {} : { timeStyle: "medium" }),
      timeZone: calendar ? "UTC" : options.timeZone,
    });
    dateFormatters.set(key, formatter);
  }
  return formatter;
}

export function fieldLabel(
  entity: string,
  field: string,
  fallback: string,
): string {
  return options.fieldLabels?.[entity]?.[field] ?? fallback;
}
