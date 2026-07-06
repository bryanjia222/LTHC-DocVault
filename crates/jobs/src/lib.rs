#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportJob {
    pub document_name: String,
    pub status: JobStatus,
}

impl ImportJob {
    pub fn pending(document_name: impl Into<String>) -> Self {
        Self {
            document_name: document_name.into(),
            status: JobStatus::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_pending_import_job() {
        let job = ImportJob::pending("contract");

        assert_eq!(job.document_name, "contract");
        assert_eq!(job.status, JobStatus::Pending);
    }
}
