import { valueError, formatMoney } from "./field-values";
import { fieldErrors, errorMessage } from "./resource";
import { afterEach, expect, test } from "@rstest/core";
import { cleanup, render, screen } from "@testing-library/react";
import {
  AppBrand,
  configureUI,
  uiText,
  t,
  formatDate,
  formatDateTime,
  formatNumber,
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
