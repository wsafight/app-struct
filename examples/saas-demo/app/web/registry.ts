import { defineAppStructRegistry } from "../../generated/web/src/generated/registry";
import { LandingPage, PricingPage } from "./ProductPages";
import { ReportStudioPage } from "./ReportStudioPage";
import { PlanningPage } from "./PlanningPage";
import { ApprovalsPage } from "./ApprovalsPage";
import { AiWorkspacePage } from "./AiWorkspacePage";

export const registry = defineAppStructRegistry({
  fields: {},
  pages: {
    LandingPage,
    PricingPage,
    ReportStudioPage,
    PlanningPage,
    ApprovalsPage,
    AiWorkspacePage,
  },
});
