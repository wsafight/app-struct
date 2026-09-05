use proc_macro2::TokenStream;
use quote::quote;

pub(super) fn source() -> TokenStream {
    quote! {
        fn render_capture_pdf(
            template: &str,
            input: &serde_json::Value,
            locale: &str,
            timezone: &str,
            paper: &str,
            orientation: &str,
        ) -> Result<Vec<u8>, String> {
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
