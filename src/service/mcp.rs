use super::*;
use crate::json::{fields, required};
const VERSIONS: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) async fn mcp(&self, request: &Request) -> Response {
        if request
            .headers
            .get("origin")
            .is_some_and(|o| o != &self.config.public_origin)
        {
            return rpc_error(403, Value::Null, -32001, "Invalid Origin.");
        }
        let principal = match self.authenticate(request).await {
            Ok(p) => p,
            Err(e) => return rpc_error(e.status(), Value::Null, -32001, &e.message),
        };
        if request.method != "POST" {
            let mut response = rpc_error(
                405,
                Value::Null,
                -32600,
                "This stateless endpoint accepts POST only.",
            );
            response.headers.insert("allow".into(), "POST".into());
            return response;
        }
        if request
            .headers
            .get("mcp-protocol-version")
            .is_some_and(|v| !VERSIONS.contains(&v.as_str()))
        {
            return rpc_error(
                400,
                Value::Null,
                -32600,
                "Unsupported MCP-Protocol-Version.",
            );
        }
        let value = match crate::json::parse(&request.body, self.limits.max_json_bytes) {
            Ok(v) => v,
            Err(_) => return rpc_error(400, Value::Null, -32700, "Invalid JSON."),
        };
        let id = value.get("id").cloned().unwrap_or(Value::Null);
        if !value.is_object()
            || value["jsonrpc"] != "2.0"
            || !value.get("method").is_some_and(Value::is_string)
            || (value.get("id").is_some() && !id.is_string() && !id.is_number())
            || fields(&value, &["jsonrpc", "id", "method", "params"]).is_err()
        {
            return rpc_error(
                400,
                Value::Null,
                -32600,
                "Expected one JSON-RPC request or notification.",
            );
        }
        let method = value["method"].as_str().unwrap_or_default();
        let params = value.get("params").cloned().unwrap_or_else(|| json!({}));
        if !params.is_object() {
            return rpc_error(400, id, -32602, "Parameters must be an object.");
        }
        if value.get("id").is_none() {
            return Response {
                status: 202,
                headers: BTreeMap::new(),
                body: vec![],
            };
        }
        let result = match method {
            "initialize" => {
                let Some(version) = params.get("protocolVersion").and_then(Value::as_str) else {
                    return rpc_error(200, id, -32602, "protocolVersion is required.");
                };
                let version = if VERSIONS.contains(&version) {
                    version
                } else {
                    VERSIONS[0]
                };
                json!({"protocolVersion":version,"capabilities":{"tools":{"listChanged":false}},"serverInfo":{"name":"waypost","version":env!("CARGO_PKG_VERSION")},"instructions":"Use waypost_describe first. Persist stable request IDs before writes and persist inbox content before acknowledging. Blob bytes use authenticated HTTP PUT/GET URLs returned by the tools."})
            }
            "ping" => json!({}),
            "tools/list" => json!({"tools":tools()}),
            "tools/call" => {
                if fields(&params, &["name", "arguments", "_meta"]).is_err() {
                    return rpc_error(200, id, -32602, "Invalid tools/call parameters.");
                }
                let Some(name) = params.get("name").and_then(Value::as_str) else {
                    return rpc_error(200, id, -32602, "Tool name is required.");
                };
                if !tools().iter().any(|t| t["name"] == name) {
                    return rpc_error(200, id, -32602, "Unknown tool.");
                }
                let arguments = params
                    .get("arguments")
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                let outcome = self.call_tool(&principal, name, arguments).await;
                let envelope = match outcome {
                    Ok(data) => json!({"ok":true,"data":data}),
                    Err(e) => json!({"ok":false,"error":e.value()}),
                };
                json!({"content":[{"type":"text","text":envelope.to_string()}],"structuredContent":envelope,"isError":envelope["ok"]!=true})
            }
            _ => return rpc_error(200, id, -32601, "Method not found."),
        };
        Response::json(200, json!({"jsonrpc":"2.0","id":id,"result":result}))
    }
    async fn call_tool(&self, p: &Principal, name: &str, args: Value) -> Result<Value> {
        match name {
            "waypost_describe" => {
                fields(&args, &[])?;
                self.describe(p).await
            }
            "waypost_send" => self.send(p, args).await,
            "waypost_message_get" => {
                fields(&args, &["message_id"])?;
                self.message(p, &required(&args, "message_id")?).await
            }
            "waypost_inbox" => self.inbox(p, args).await,
            "waypost_ack" => self.ack(p, args).await,
            "waypost_status" => self.status(p, args).await,
            "waypost_blob_prepare" => self.blob_prepare(p, args).await,
            "waypost_blob_commit" => {
                self.blob_commit(p, &required(&args, "upload_id")?, args)
                    .await
            }
            "waypost_blob_get" => {
                fields(&args, &["blob_id"])?;
                self.blob_get(p, &required(&args, "blob_id")?).await
            }
            "waypost_memory_publish" => {
                self.memory_publish(p, &required(&args, "collection_id")?, args)
                    .await
            }
            "waypost_memory_read" => {
                self.memory_read(p, &required(&args, "collection_id")?, args)
                    .await
            }
            _ => Err(Error::not_found()),
        }
    }
}
fn rpc_error(status: u16, id: Value, code: i32, message: &str) -> Response {
    Response::json(
        status,
        json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}),
    )
}
fn schema(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn tools() -> Vec<Value> {
    let s = json!({"type":"string"});
    let strings = json!({"type":"array","items":{"type":"string"}});
    let n = json!({"type":"integer","minimum":0});
    let nullable = json!({"type":["string","null"]});
    let item = schema(
        json!({"item_id":s,"blob_id":s,"path":s,"metadata":{"type":"object"}}),
        &["item_id", "blob_id", "path"],
    );
    let definitions = vec![
        (
            "waypost_describe",
            "Inspect identity, grants, limits and supported capabilities.",
            schema(json!({}), &[]),
            true,
        ),
        (
            "waypost_send",
            "Durably send an immutable message with a stable request ID.",
            schema(
                json!({"request_id":s,"recipient_ids":strings,"thread_id":nullable,"in_reply_to":nullable,"kind":{"type":"string","enum":["info","result","question","todo_suggestion","memory_notice","processing_result"]},"subject":s,"body":schema(json!({"text":s,"options":{"type":"array","items":{"type":"string"}}}),&["text"]),"source":schema(json!({"native_thread_id":nullable,"native_message_id":nullable,"url":nullable}),&[]),"attachment_ids":strings,"metadata":{"type":"object"}}),
                &["request_id", "kind", "subject", "body"],
            ),
            false,
        ),
        (
            "waypost_message_get",
            "Read a message currently visible to this Actor.",
            schema(json!({"message_id":s}), &["message_id"]),
            true,
        ),
        (
            "waypost_inbox",
            "Read a Consumer feed page without acknowledging it.",
            schema(
                json!({"consumer_id":s,"cursor":nullable,"limit":n}),
                &["consumer_id"],
            ),
            true,
        ),
        (
            "waypost_ack",
            "Confirm inputs only after local durable storage; stages are monotonic.",
            schema(
                json!({"request_id":s,"consumer_id":s,"checkpoint_cursor":nullable,"items":{"type":"array","items":schema(json!({"item_id":s,"stage":{"type":"string","enum":["received","processed"]}}),&["item_id","stage"])}}),
                &["request_id", "consumer_id"],
            ),
            false,
        ),
        (
            "waypost_status",
            "Recover the original outcome of an Actor-stable submission.",
            schema(
                json!({"operation":s,"request_id":s}),
                &["operation", "request_id"],
            ),
            true,
        ),
        (
            "waypost_blob_prepare",
            "Reserve a create-only upload. Upload exact bytes to upload_url before commit.",
            schema(
                json!({"request_id":s,"content_type":s,"size_bytes":n,"sha256":s,"filename":nullable}),
                &["request_id", "content_type", "size_bytes", "sha256"],
            ),
            false,
        ),
        (
            "waypost_blob_commit",
            "Seal verified uploaded bytes; preserve this request ID on retries.",
            schema(
                json!({"request_id":s,"upload_id":s}),
                &["request_id", "upload_id"],
            ),
            false,
        ),
        (
            "waypost_blob_get",
            "Get authorized Blob metadata and an authenticated download URL.",
            schema(json!({"blob_id":s}), &["blob_id"]),
            true,
        ),
        (
            "waypost_memory_publish",
            "Publish a full snapshot or partial delta using expected_revision. Never use snapshot for a partial export.",
            schema(
                json!({"request_id":s,"collection_id":s,"expected_revision":n,"mode":{"type":"string","enum":["snapshot","delta"]},"items":{"type":"array","items":item},"delete_ids":strings,"manifest_blob_id":s}),
                &["request_id", "collection_id", "expected_revision", "mode"],
            ),
            false,
        ),
        (
            "waypost_memory_read",
            "Read a fixed memory revision; retain next_cursor for stable pagination and export.",
            schema(
                json!({"collection_id":s,"revision":{"type":["integer","null"],"minimum":0},"limit":n,"cursor":nullable,"include_tombstones":{"type":"boolean"}}),
                &["collection_id"],
            ),
            true,
        ),
    ];
    definitions.into_iter().map(|(name,description,input,read_only)|json!({"name":name,"description":description,"inputSchema":input,"annotations":{"readOnlyHint":read_only,"destructiveHint":!read_only,"idempotentHint":true,"openWorldHint":false}})).collect()
}
