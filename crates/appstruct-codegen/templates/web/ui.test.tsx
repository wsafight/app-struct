import { valueError, formatMoney } from "./field-values";
import { fieldErrors, errorMessage } from "./resource";
import { afterEach, expect, test } from "@rstest/core";
import {
  act,
  cleanup,
  fireEvent,
  render,
  renderHook,
  screen,
} from "@testing-library/react";
import {
  AppBrand,
  configureUI,
  uiText,
  t,
  formatDate,
  formatDateTime,
  formatNumber,
  formatMinorMoney,
  statusLabel,
  LoadingState,
  EmptyState,
  ActionFeedback,
  useActionFeedback,
} from "./ui";

afterEach(() => {
  cleanup();
  configureUI({});
});

test("user branding and messages replace defaults without changing resource labels", () => {
  configureUI({
    title: "客户后台",
    logo: { src: "/brand.svg", alt: "Company" },
    messages: { editRecord: "编辑 {resource}", save: "保存" },
  });
  render(<AppBrand fallbackTitle="Default app" />);
  expect(screen.getByText("客户后台")).toBeTruthy();
  expect(screen.getByRole("img", { name: "Company" }).getAttribute("src")).toBe(
    "/brand.svg",
  );
  expect(document.title).toBe("客户后台");
  expect(uiText("editRecord", { resource: "订单" })).toBe("编辑 订单");
  expect(uiText("save")).toBe("保存");
  expect(uiText("cancel")).toBe("Cancel");
});

test("theme configuration applies palette overrides and clears replaced values", () => {
  configureUI({ colorScheme: "dark", colors: { accent: "#3344aa" } });
  expect(document.documentElement.dataset.theme).toBe("dark");
  expect(
    document.documentElement.style.getPropertyValue("--color-accent"),
  ).toBe("#3344aa");
  configureUI({ colorScheme: "light" });
  expect(document.documentElement.dataset.theme).toBe("light");
  expect(
    document.documentElement.style.getPropertyValue("--color-accent"),
  ).toBe("");
});

test("Chinese copy, validation and configured display preserve business values and precision", () => {
  configureUI({
    locale: "zh-CN",
    timeZone: "Asia/Shanghai",
    messages: { save: "提交" },
  });
  expect(document.documentElement.lang).toBe("zh-CN");
  expect(uiText("save")).toBe("提交");
  expect(uiText("editRecord", { resource: "订单" })).toBe("编辑订单");
  expect(t("Compare changes")).toBe("对比变更");
  expect(t("Check import result")).toBe("查询导入结果");
  expect(t("My custom business value")).toBe("My custom business value");
  expect(formatNumber("9007199254740993.15")).toBe("9,007,199,254,740,993.15");
  expect(formatDateTime("2026-01-01T18:00:00Z")).toContain("2026年1月2日");
  expect(formatDate("2026-01-01")).toBe("2026年1月1日");
  expect(formatMoney("9007199254740993.15", "CNY", 2)).toContain(
    "9,007,199,254,740,993.15",
  );
  expect(
    valueError("", {
      name: "name",
      label: "姓名",
      kind: "string",
      required: true,
    }),
  ).toBe("姓名为必填项");
  expect(
    fieldErrors({
      fields: [
        { field: "name", message: "must contain at least 2 characters" },
      ],
    }),
  ).toEqual({ name: "至少需要2个字符" });
  expect(errorMessage(new Error("Too many authentication attempts"))).toBe(
    "认证尝试过于频繁，请稍后重试",
  );
});

test("product statuses and money are localized without losing integer precision", () => {
  expect(formatMinorMoney("900719925474099315", "USD")).toContain(
    "9,007,199,254,740,993.15",
  );
  expect(formatMinorMoney("123", "JPY")).not.toContain(".00");
  expect(formatMinorMoney("1234", "KWD")).toContain("1.234");
  expect(formatMinorMoney("-100", "USD")).toContain("1.00");
  expect(formatMinorMoney("100", "ISK")).toContain("1.00");
  expect(formatMinorMoney("100", "UGX")).toContain("1.00");
  expect(formatMinorMoney("9007199254740993123456", "USD")).toContain(
    "90,071,992,547,409,931,234.56",
  );
  expect(formatMinorMoney("123", "invalid")).toBe("123 INVALID (minor units)");
  configureUI({ locale: "zh-CN" });
  expect(statusLabel("in_progress")).toBe("进行中");
  expect(statusLabel("approved")).toBe("已同意");
  expect(statusLabel("custom-business-state")).toBe("custom-business-state");
  for (const copy of [
    "Notifications",
    "Team settings",
    "AI writing assistant",
    "Month grid",
    "Copy recovery codes",
    "Team and account",
  ])
    expect(t(copy)).not.toBe(copy);
  expect(t("Submitted {0}", { "0": "2026年11月5日" })).toBe(
    "提交于 2026年11月5日",
  );
});

test("shared loading, empty and retry states expose actionable accessible copy", () => {
  let retried = 0;
  render(
    <>
      <LoadingState label="Loading tasks" />
      <EmptyState title="No results" description="Create a report first">
        <a href="/reports">Create report</a>
      </EmptyState>
      <ActionFeedback
        error="Connection failed"
        notice="Saved"
        onRetry={() => {
          retried += 1;
        }}
      />
    </>,
  );
  expect(screen.getByRole("status", { name: "Loading tasks" })).toBeTruthy();
  expect(screen.getByRole("heading", { name: "No results" })).toBeTruthy();
  expect(screen.getByRole("link", { name: "Create report" })).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Try again" }));
  expect(retried).toBe(1);
  expect(screen.getByRole("alert").textContent).toContain("Connection failed");
});

test("row actions deduplicate the same row but do not block independent rows", async () => {
  const { result } = renderHook(() => useActionFeedback(errorMessage));
  let finishFirst: () => void = () => undefined;
  const operation = new Promise<void>((resolve) => {
    finishFirst = resolve;
  });
  let first: Promise<boolean> = Promise.resolve(false);
  act(() => {
    first = result.current.run("first", () => operation, "First saved");
  });
  expect(result.current.isPending("first")).toBe(true);
  expect(result.current.isPending("second")).toBe(false);
  await act(async () => {
    expect(
      await result.current.run("first", async () => {
        throw new Error("must not execute");
      }),
    ).toBe(false);
  });
  await act(async () => {
    expect(
      await result.current.run("second", async () => undefined, "Second saved"),
    ).toBe(true);
  });
  expect(result.current.isPending("first")).toBe(true);
  expect(result.current.notice).toBe("Second saved");
  await act(async () => {
    finishFirst();
    await first;
  });
  expect(result.current.isPending("first")).toBe(false);
  await act(async () => {
    expect(
      await result.current.run("failed", async () => {
        throw new Error("Try later");
      }),
    ).toBe(false);
  });
  expect(result.current.error).toBe("Try later");
  expect(result.current.isPending("failed")).toBe(false);
});
