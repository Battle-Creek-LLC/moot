//! Live Recall.ai integration test. Skipped unless MOOT_RECALL_INTEGRATION=1
//! is set. Exercises the auth path (`check`) using whatever key is in the
//! keychain or `MOOT_RECALL_API_KEY`.

use moot::recall::{DEFAULT_REGION, RecallApi, RecallClient};

#[tokio::test]
async fn auth_check_against_real_recall() {
    if std::env::var_os("MOOT_RECALL_INTEGRATION").is_none() {
        eprintln!("skipped: set MOOT_RECALL_INTEGRATION=1 to run");
        return;
    }
    let key = moot::secrets::get().expect("no Recall.ai key available");
    let region = std::env::var("MOOT_RECALL_REGION").unwrap_or_else(|_| DEFAULT_REGION.into());
    let client = RecallClient::new(&key, &region);
    client.check().await.expect("check failed");
}
