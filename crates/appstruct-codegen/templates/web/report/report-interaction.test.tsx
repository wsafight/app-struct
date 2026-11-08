import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, rs } from "@rstest/core";
import * as clientActual from "../generated/client" with {
  rstest: "importActual",
};
import type { ReportRun, ReportTemplate } from "../generated/client";
import { ReportPage } from "./ReportPage";

const reportMocks = rs.hoisted(() => ({
  templates: rs.fn(),
  list: rs.fn(),
  create: rs.fn(),
  download: rs.fn(),
}));

rs.mock("../generated/client", () => ({
  ...clientActual,
  reportApi: {
    ...clientActual.reportApi,
    templates: reportMocks.templates,
    list: reportMocks.list,
    create: reportMocks.create,
    download: reportMocks.download,
  },
}));

const templateName = __REPORT_TEMPLATE_NAME__;

afterEach(cleanup);

beforeEach(() => {
  reportMocks.templates.mockReset();
  reportMocks.list.mockReset();
  reportMocks.create.mockReset();
  reportMocks.download.mockReset();
  reportMocks.list.mockResolvedValue({
    data: [],
    meta: { page: 1, page_size: 25, total: 0 },
  });
  reportMocks.create.mockResolvedValue(reportRun());
});

describe("report inputs", () => {
  it("keeps empty numbers empty and submits an untouched required boolean as false", async () => {
    reportMocks.templates.mockResolvedValue([
      reportTemplate({
        required: ["approved", "count"],
        properties: {
          approved: { type: "boolean" },
          count: { type: "integer" },
        },
      }),
    ]);
    const user = userEvent.setup();
    renderPage();

    await selectTemplate(user);
    const generate = screen.getByRole("button", { name: "Generate" });
    const count = screen.getByLabelText("count") as HTMLInputElement;
    expect((generate as HTMLButtonElement).disabled).toBe(true);

    await user.type(count, "12");
    expect((generate as HTMLButtonElement).disabled).toBe(false);
    await user.clear(count);
    expect(count.value).toBe("");
    expect((generate as HTMLButtonElement).disabled).toBe(true);

    await user.type(count, "0");
    await user.click(generate);
    await waitFor(() => expect(reportMocks.create).toHaveBeenCalledOnce());
    expect(reportMocks.create.mock.calls[0][1]).toEqual({
      approved: false,
      count: 0,
    });
  });

  it("omits a cleared optional number instead of submitting zero", async () => {
    reportMocks.templates.mockResolvedValue([
      reportTemplate({
        required: ["approved"],
        properties: {
          approved: { type: "boolean" },
          count: { type: "number" },
        },
      }),
    ]);
    const user = userEvent.setup();
    renderPage();

    await selectTemplate(user);
    const count = screen.getByLabelText("count");
    await user.type(count, "4");
    await user.clear(count);
    const generate = screen.getByRole("button", { name: "Generate" });
    expect((generate as HTMLButtonElement).disabled).toBe(false);
    await user.click(generate);

    await waitFor(() => expect(reportMocks.create).toHaveBeenCalledOnce());
    expect(reportMocks.create.mock.calls[0][1]).toEqual({ approved: false });
  });

  it("submits string enum values with their declared type", async () => {
    reportMocks.templates.mockResolvedValue([
      reportTemplate({
        required: ["mode"],
        properties: {
          mode: { type: "string", enum: ["fast", "full"] },
        },
      }),
    ]);
    const user = userEvent.setup();
    renderPage();

    await selectTemplate(user);
    await user.selectOptions(screen.getByLabelText("mode"), "full");
    await user.click(screen.getByRole("button", { name: "Generate" }));

    await waitFor(() => expect(reportMocks.create).toHaveBeenCalledOnce());
    expect(reportMocks.create.mock.calls[0][1]).toEqual({ mode: "full" });
  });

  it("explicitly rejects object and array inputs instead of sending strings", async () => {
    reportMocks.templates.mockResolvedValue([
      reportTemplate({
        properties: {
          filters: { type: "object", properties: {} },
          tags: { type: "array", items: { type: "string" } },
        },
      }),
    ]);
    const user = userEvent.setup();
    renderPage();

    await selectTemplate(user);
    expect(screen.getAllByText("Unsupported report input type")).toHaveLength(
      2,
    );
    expect(
      (screen.getByRole("button", { name: "Generate" }) as HTMLButtonElement)
        .disabled,
    ).toBe(true);
    expect(reportMocks.create).not.toHaveBeenCalled();
  });
});

it("uses the shared safe error parser for report downloads", async () => {
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () =>
    new Response(JSON.stringify({ error: null }), { status: 500 });
  try {
    const error = await clientActual.reportApi
      .download("run-1")
      .catch((reason: unknown) => reason);
    expect(error).toBeInstanceOf(clientActual.ApiError);
    expect(error).toMatchObject({
      status: 500,
      code: "HTTP_ERROR",
      message: "Request failed with status 500",
    });
  } finally {
    globalThis.fetch = originalFetch;
  }
});

async function selectTemplate(user: ReturnType<typeof userEvent.setup>) {
  const select = await screen.findByLabelText("Template");
  await screen.findByRole("option", { name: `${templateName} v1` });
  await user.selectOptions(select, templateName);
}

function renderPage() {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={client}>
      <ReportPage />
    </QueryClientProvider>,
  );
}

function reportTemplate(inputSchema: Record<string, unknown>): ReportTemplate {
  return {
    name: templateName,
    version: 1,
    document_type: "pdf",
    artifact_digest: "sha256:test",
    input_schema: { type: "object", ...inputSchema },
    data_schema_version: 1,
    renderer_version: "capture-v1",
  };
}

function reportRun(): ReportRun {
  return {
    id: "run-1",
    execution_job_id: null,
    template: templateName,
    template_version: 1,
    tenant_id: null,
    actor_id: null,
    stage: "queued",
    progress: 0,
    locale: "en-US",
    timezone: "UTC",
    paper: "a4",
    orientation: "portrait",
    result_file_id: null,
    error_code: null,
    created_at: "2026-01-01T00:00:00Z",
    completed_at: null,
    expires_at: "2026-01-02T00:00:00Z",
  };
}
