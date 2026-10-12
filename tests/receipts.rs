mod common;
use common::*;
use serde_json::json;

#[tokio::test(flavor = "current_thread")]
async fn processed_receipts_are_monotonic_and_acknowledging_a_receipt_does_not_create_a_loop() {
    let f = Fixture::new().await;
    f.send("first-message", "Persistent input").await;
    let page = f.inbox().await;
    let item = page["items"][0]["item_id"].clone();
    let ack = json!({"request_id":"processed-ack","consumer_id":f.space["default_consumer_id"],"items":[{"item_id":item,"stage":"processed"}],"checkpoint_cursor":page["next_cursor"]});
    let first = call(&f.app, "POST", "/v0/receipts", &f.reader_token, ack.clone()).await;
    let repeated = call(&f.app, "POST", "/v0/receipts", &f.reader_token, ack).await;
    assert_eq!(first, repeated);
    call(&f.app,"POST","/v0/receipts",&f.reader_token,json!({"request_id":"late-received","consumer_id":f.space["default_consumer_id"],"items":[{"item_id":item,"stage":"received"}]})).await;
    let receipts = f.writer_inbox().await;
    assert_eq!(receipts["items"].as_array().unwrap().len(), 1);
    assert_eq!(receipts["items"][0]["data"]["stage"], "processed");
    call(&f.app,"POST","/v0/receipts",&f.writer_token,json!({"request_id":"receipt-of-receipt","consumer_id":f.writer_consumer["consumer_id"],"items":[{"item_id":receipts["items"][0]["item_id"],"stage":"received"}]})).await;
    assert_eq!(f.inbox().await["items"].as_array().unwrap().len(), 1);
}
