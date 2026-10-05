use crate::VERSION;

pub async fn healthz() -> String {
    format!("Running OK - v{}", VERSION)
}