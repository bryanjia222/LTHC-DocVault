use std::path::Path;

const SUPPORTED_EXTENSIONS: &[&str] = &["docx", "xlsx", "pptx"];

pub fn is_supported_ooxml(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            SUPPORTED_EXTENSIONS
                .iter()
                .any(|supported| extension.eq_ignore_ascii_case(supported))
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_supported_office_documents() {
        assert!(is_supported_ooxml("report.docx"));
        assert!(is_supported_ooxml("sheet.XLSX"));
        assert!(is_supported_ooxml("deck.pptx"));
        assert!(!is_supported_ooxml("notes.txt"));
    }
}
