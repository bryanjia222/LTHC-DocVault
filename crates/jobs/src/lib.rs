use tracing::info;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitJob {
    pub document_name: String,
    pub status: JobStatus,
}

impl CommitJob {
    pub fn pending(document_name: impl Into<String>) -> Self {
        let document_name = document_name.into();
        info!(
            document_name = document_name.as_str(),
            "creating pending commit job"
        );
        Self {
            document_name,
            status: JobStatus::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_pending_commit_job() {
        let job = CommitJob::pending("contract");

        assert_eq!(job.document_name, "contract");
        assert_eq!(job.status, JobStatus::Pending);
    }
}
