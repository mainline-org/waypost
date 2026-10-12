mod common;
use common::*;
use serde_json::{Value, json};
use waypost::Request;
#[tokio::test(flavor = "current_thread")]
async fn stateless_mcp_negotiates_and_exposes_the_same_durable_tools() {
    let f = Fixture::new().await;
    let r=f.app.handle(Request::json("POST","/mcp",&f.writer_token,json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}))).await;
    assert_eq!(r.status, 200);
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["result"]["protocolVersion"], "2025-11-25");
    let mut notification = Request::json(
        "POST",
        "/mcp",
        &f.writer_token,
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    );
    notification
        .headers
        .insert("mcp-protocol-version".into(), "2025-11-25".into());
    let r = f.app.handle(notification).await;
    assert_eq!(r.status, 202);
    assert!(r.body.is_empty());
    let mut req = Request::json(
        "POST",
        "/mcp",
        &f.writer_token,
        json!({"jsonrpc":"2.0","id":"tools","method":"tools/list"}),
    );
    req.headers
        .insert("mcp-protocol-version".into(), "2025-11-25".into());
    let r = f.app.handle(req).await;
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["result"]["tools"].as_array().unwrap().len(), 11);
    let mut req = Request::json(
        "POST",
        "/mcp",
        &f.writer_token,
        json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"waypost_send","arguments":{"request_id":"mcp-send","kind":"info","subject":"MCP","body":{"text":"From a real tool call"}}}}),
    );
    req.headers
        .insert("mcp-protocol-version".into(), "2025-11-25".into());
    let r = f.app.handle(req).await;
    let v: Value = serde_json::from_slice(&r.body).unwrap();
    assert_eq!(v["result"]["isError"], false, "{v}");
    let id = v["result"]["structuredContent"]["data"]["message_id"]
        .as_str()
        .unwrap();
    let message = call(
        &f.app,
        "GET",
        &format!("/v0/messages/{id}"),
        &f.reader_token,
        json!({}),
    )
    .await;
    assert_eq!(message["body"]["text"], "From a real tool call");
    let r = f
        .app
        .handle(Request::json("GET", "/mcp", &f.writer_token, json!({})))
        .await;
    assert_eq!(r.status, 405);
}
