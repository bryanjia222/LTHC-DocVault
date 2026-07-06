use docvault_types::{Document, DocumentId};

pub fn register_document(name: impl Into<String>, source_path: impl Into<String>) -> Document {
    Document {
        id: DocumentId::new(name.into()),
        source_path: source_path.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registers_document_with_domain_id() {
        let document = register_document("report", "./report.docx");

        assert_eq!(document.id.as_str(), "report");
        assert_eq!(document.source_path, "./report.docx");
    }
}
