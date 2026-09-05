use appstruct_ir::AppIr;
use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source(ir: &AppIr) -> Option<TokenStream> {
    if !ir.mail.enabled && !ir.report.enabled {
        return None;
    }
    let mail_field = ir.mail.enabled.then(|| quote! { mail: MailJobHandler, });
    let mail_init = ir
        .mail
        .enabled
        .then(|| quote! { mail: MailJobHandler::new(mail), });
    let mail_arm = ir.mail.enabled.then(|| {
        quote! { if job.kind == "mail.send" { return self.mail.handle(job).await; } }
    });
    let report_field = ir
        .report
        .enabled
        .then(|| quote! { report: crate::report::ReportJobHandler, });
    let report_init = ir.report.enabled.then(|| {
        quote! { report: crate::report::ReportJobHandler::new(database, file), }
    });
    let report_arm = ir.report.enabled.then(|| {
        quote! {
            if matches!(job.kind.as_str(), "report.render" | "report.cleanup") {
                return self.report.handle(job).await;
            }
        }
    });
    let unused_database = (!ir.report.enabled).then(|| quote! { let _ = database; });
    let unused_mail = (!ir.mail.enabled).then(|| quote! { let _ = mail; });
    let unused_file = (!ir.report.enabled).then(|| quote! { let _ = file; });
    Some(quote! {
        struct GeneratedJobHandler {
            custom: Option<Arc<dyn JobHandler>>,
            #mail_field
            #report_field
        }
        pub(crate) fn generated_job_handler(
            database: DatabaseConnection,
            mail: crate::MailState,
            file: crate::FileState,
            custom: Option<Arc<dyn JobHandler>>,
        ) -> Arc<dyn JobHandler> {
            #unused_database #unused_mail #unused_file
            Arc::new(GeneratedJobHandler { custom, #mail_init #report_init })
        }
        #[async_trait]
        impl JobHandler for GeneratedJobHandler {
            async fn handle(&self, job: &Job) -> Result<(), JobHandlerError> {
                #mail_arm
                #report_arm
                if let Some(custom) = &self.custom { return custom.handle(job).await; }
                Err(JobHandlerError(format!("unsupported job kind `{}`", job.kind)))
            }
        }
    })
}
