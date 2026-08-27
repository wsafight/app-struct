use appstruct_ir::Diagnostic;

#[derive(Default)]
pub(super) struct DecodeContext {
    diagnostics: Vec<Diagnostic>,
}

impl DecodeContext {
    pub(super) fn capture<T>(&mut self, result: Result<T, Diagnostic>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(diagnostic) => {
                self.diagnostics.push(diagnostic);
                None
            }
        }
    }

    pub(super) fn capture_many<T>(&mut self, result: Result<T, Vec<Diagnostic>>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(mut diagnostics) => {
                self.diagnostics.append(&mut diagnostics);
                None
            }
        }
    }

    pub(super) fn extend(&mut self, diagnostics: impl IntoIterator<Item = Diagnostic>) {
        self.diagnostics.extend(diagnostics);
    }

    pub(super) fn finish<T>(mut self, value: Option<T>) -> Result<T, Vec<Diagnostic>> {
        self.diagnostics.sort_by(|left, right| {
            left.primary
                .span
                .file
                .cmp(&right.primary.span.file)
                .then(left.primary.span.start.cmp(&right.primary.span.start))
                .then(left.code.cmp(&right.code))
                .then(left.message.cmp(&right.message))
        });
        if self.diagnostics.is_empty() {
            Ok(value.expect("shape decoding without diagnostics produced a value"))
        } else {
            Err(self.diagnostics)
        }
    }
}
