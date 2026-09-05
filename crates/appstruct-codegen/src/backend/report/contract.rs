use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source(ir: &AppIr) -> TokenStream {
    let max_input_bytes = ir.report.max_input_bytes;
    let retention_days = ir.report.retention_days;
    let queue = &ir.report.queue;
    let reader_roles = &ir.report.reader_roles;
    let templates = ir.report.templates.iter().map(|template| {
        let name = &template.name;
        let version = template.version;
        let body = &template.body;
        let artifact_digest = &template.artifact_digest;
        let input_schema = &template.input_schema;
        let data_schema_version = template.data_schema_version;
        quote! {
            ReportTemplateConfig {
                name: #name, version: #version, body: #body,
                artifact_digest: #artifact_digest, input_schema: #input_schema,
                data_schema_version: #data_schema_version,
            }
        }
    });
    quote! {
        const REPORT_QUEUE: &str = #queue;
        const REPORT_MAX_INPUT_BYTES: u64 = #max_input_bytes;
        const REPORT_RETENTION_DAYS: u32 = #retention_days;
        const REPORT_RENDERER_VERSION: &str = "capture-v1";

        #[derive(Clone, Copy)]
        struct ReportTemplateConfig {
            name: &'static str,
            version: u32,
            body: &'static str,
            artifact_digest: &'static str,
            input_schema: &'static str,
            data_schema_version: u32,
        }
        const REPORT_TEMPLATES: &[ReportTemplateConfig] = &[#(#templates),*];

        #[derive(Clone, Debug, Serialize)]
        pub struct ReportTemplate {
            pub name: String,
            pub version: u32,
            pub document_type: String,
            pub artifact_digest: String,
            pub input_schema: serde_json::Value,
            pub data_schema_version: u32,
            pub renderer_version: String,
        }

        #[derive(Clone, Debug, Serialize)]
        pub struct ReportRun {
            pub id: uuid::Uuid,
            pub execution_job_id: Option<uuid::Uuid>,
            pub template: String,
            pub template_version: u32,
            pub tenant_id: Option<uuid::Uuid>,
            pub actor_id: Option<uuid::Uuid>,
            pub stage: String,
            pub progress: u32,
            pub locale: String,
            pub timezone: String,
            pub paper: String,
            pub orientation: String,
            pub result_file_id: Option<uuid::Uuid>,
            pub error_code: Option<String>,
            pub created_at: chrono::DateTime<chrono::Utc>,
            pub completed_at: Option<chrono::DateTime<chrono::Utc>>,
            pub expires_at: chrono::DateTime<chrono::Utc>,
        }

        #[derive(Debug, Deserialize)]
        struct CreateReportRun {
            data: serde_json::Value,
            locale: Option<String>,
            timezone: Option<String>,
            paper: Option<String>,
            orientation: Option<String>,
        }

        #[derive(Debug, Serialize, Deserialize)]
        pub struct ReportJobPayload { pub run_id: uuid::Uuid }

        #[derive(Debug, Default, Deserialize)]
        struct ReportRunQuery { page: Option<u64>, page_size: Option<u64> }
        #[derive(Debug, Serialize)]
        struct ReportRunList { data: Vec<ReportRun>, meta: ReportRunListMeta }
        #[derive(Debug, Serialize)]
        struct ReportRunListMeta { page: u64, page_size: u64, total: u64 }

        fn template_config(name: &str) -> Option<ReportTemplateConfig> {
            REPORT_TEMPLATES.iter().copied().find(|template| template.name == name)
        }
        fn can_read_all_reports(context: &RequestContext<'_>) -> bool {
            context.actor().is_some_and(|actor| false #(|| actor.has_role(#reader_roles))* )
        }
    }
}
