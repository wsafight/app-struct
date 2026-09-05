use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source() -> TokenStream {
    quote! {
        fn render_capture_pdf(
            run_id: uuid::Uuid,
            template: &str,
            input: &serde_json::Value,
            locale: &str,
            timezone: &str,
            paper: &str,
            orientation: &str,
        ) -> Result<Vec<u8>, String> {
            test_report_failure(run_id, input)?;
            let environment = minijinja::Environment::new();
            let rendered = environment.render_str(
                template,
                minijinja::context! {
                    input => input,
                    locale => locale,
                    timezone => timezone,
                    paper => paper,
                    orientation => orientation,
                },
            ).map_err(|_| "REPORT_TEMPLATE_RENDER_FAILED".to_owned())?;
            Ok(minimal_pdf(&rendered))
        }

        #[cfg(feature = "test-support")]
        fn test_report_failure(
            run_id: uuid::Uuid, input: &serde_json::Value,
        ) -> Result<(), String> {
            let Some(marker) = std::env::var_os("APPSTRUCT_TEST_REPORT_FAIL_ONCE") else {
                return Ok(());
            };
            let marker = marker.to_string_lossy();
            if !test_value_contains(input, &marker) { return Ok(()); }
            static FAILED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<uuid::Uuid>>> =
                std::sync::OnceLock::new();
            let mut failed = FAILED.get_or_init(Default::default).lock()
                .map_err(|_| "REPORT_TEST_SUPPORT_FAILED".to_owned())?;
            if failed.insert(run_id) {
                return Err("REPORT_TEST_INJECTED_FAILURE".to_owned());
            }
            Ok(())
        }

        #[cfg(feature = "test-support")]
        fn test_value_contains(value: &serde_json::Value, marker: &str) -> bool {
            match value {
                serde_json::Value::String(value) => value == marker,
                serde_json::Value::Array(values) =>
                    values.iter().any(|value| test_value_contains(value, marker)),
                serde_json::Value::Object(values) =>
                    values.values().any(|value| test_value_contains(value, marker)),
                _ => false,
            }
        }

        #[cfg(not(feature = "test-support"))]
        fn test_report_failure(
            _run_id: uuid::Uuid, _input: &serde_json::Value,
        ) -> Result<(), String> { Ok(()) }

        fn minimal_pdf(rendered: &str) -> Vec<u8> {
            let text = rendered.chars().take(20_000).map(|character| match character {
                '\\' => "\\\\".to_owned(),
                '(' => "\\(".to_owned(),
                ')' => "\\)".to_owned(),
                '\n' | '\r' | '\t' => " ".to_owned(),
                value if value.is_ascii() && !value.is_control() => value.to_string(),
                _ => "?".to_owned(),
            }).collect::<String>();
            let stream = format!("BT /F1 10 Tf 50 790 Td ({text}) Tj ET");
            let objects = vec![
                "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
                "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_owned(),
                "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
                format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
            ];
            let mut output = b"%PDF-1.4\n".to_vec();
            let mut offsets = Vec::with_capacity(objects.len());
            for (index, object) in objects.iter().enumerate() {
                offsets.push(output.len());
                output.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
            }
            let xref = output.len();
            output.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
            for offset in offsets {
                output.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
            }
            output.extend_from_slice(format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objects.len() + 1,
            ).as_bytes());
            output
        }
    }
}
