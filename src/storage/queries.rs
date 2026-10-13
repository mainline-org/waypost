// Operation-named SQL shared by SQLite and D1. Bound values never enter SQL text.
pub const AUTHENTICATE: &str = "SELECT t.token_id,t.space_id,t.actor_id,t.consumer_id,t.authority_version AS credential_version,a.authority_version AS actor_version,c.view,c.view_revision,s.default_actor_id,COALESCE(t.default_recipient_id,s.default_actor_id) AS default_recipient_id,
 (SELECT json_group_array(scope) FROM (SELECT scope FROM token_scopes WHERE token_id=t.token_id ORDER BY scope)) AS scopes_json,
 (SELECT json_group_array(actor_id) FROM (SELECT actor_id FROM token_recipients WHERE token_id=t.token_id AND space_id=t.space_id ORDER BY actor_id)) AS recipients_json,
 (SELECT json_group_array(json_object('collection_id',collection_id,'action',action)) FROM (SELECT collection_id,action FROM token_collections WHERE token_id=t.token_id AND space_id=t.space_id ORDER BY collection_id,action)) AS collections_json,
 (SELECT json_group_array(blob_id) FROM (SELECT blob_id FROM token_blob_grants WHERE token_id=t.token_id AND space_id=t.space_id ORDER BY blob_id)) AS blobs_json,
 (SELECT json_group_array(json_object('dimension',dimension,'value',value)) FROM (SELECT dimension,value FROM consumer_filters WHERE space_id=t.space_id AND consumer_id=t.consumer_id ORDER BY dimension,value)) AS filters_json
 FROM tokens t JOIN actors a ON a.space_id=t.space_id AND a.actor_id=t.actor_id JOIN consumers c ON c.space_id=t.space_id AND c.consumer_id=t.consumer_id AND c.actor_id=t.actor_id JOIN spaces s ON s.space_id=t.space_id
 WHERE t.token_digest=? AND t.revoked_at IS NULL AND (t.expires_at IS NULL OR t.expires_at>?) AND a.disabled=0 AND c.disabled=0";

pub const SUBMISSION_BY_REQUEST: &str =
    "SELECT * FROM submissions WHERE space_id=? AND producer_id=? AND operation=? AND request_id=?";
pub const UPLOAD_BY_OWNER: &str =
    "SELECT * FROM uploads WHERE space_id=? AND upload_id=? AND producer_id=?";

use super::driver::{Statement, Value as Sql};
pub fn guarded_inserts(
    table: &str,
    columns: &str,
    rows: Vec<Vec<Sql>>,
    guard: Option<(String, Vec<Sql>)>,
) -> Vec<Statement> {
    if rows.is_empty() {
        return vec![];
    }
    let width = rows[0].len();
    let reserved = guard.as_ref().map(|x| x.1.len()).unwrap_or(0);
    let chunk = ((100 - reserved) / width).max(1);
    let mut result = Vec::new();
    for rows in rows.chunks(chunk) {
        let mut parameters = Vec::new();
        let groups = rows
            .iter()
            .map(|r| {
                parameters.extend(r.iter().cloned());
                format!("({})", vec!["?"; width].join(","))
            })
            .collect::<Vec<_>>();
        let mut sql = format!(
            "INSERT INTO {table}({columns}) SELECT * FROM (VALUES {})",
            groups.join(",")
        );
        if let Some((g, p)) = &guard {
            sql.push_str(&format!(" WHERE {g}"));
            parameters.extend(p.iter().cloned());
        }
        result.push(Statement::new(sql, parameters));
    }
    result
}

pub const ACK_CREATE_RECEIPTS: &str = "INSERT INTO receipts(space_id,receipt_id,consumer_id,subject_item_id,stage,version,updated_at,last_submission_id) SELECT ai.space_id,ai.candidate_receipt_id,?,ai.subject_item_id,ai.desired_stage,1,?,? FROM submission_ack_items ai WHERE ai.submission_id=? AND {guard} ON CONFLICT(space_id,consumer_id,subject_item_id) DO NOTHING";

pub const ACK_ADVANCE_RECEIPTS: &str = "UPDATE receipts SET stage=(SELECT desired_stage FROM submission_ack_items ai WHERE ai.submission_id=? AND ai.subject_item_id=receipts.subject_item_id),version=version+1,updated_at=?,last_submission_id=? WHERE space_id=? AND consumer_id=? AND EXISTS(SELECT 1 FROM submission_ack_items ai WHERE ai.submission_id=? AND ai.subject_item_id=receipts.subject_item_id AND ai.desired_stage>receipts.stage) AND {guard}";

pub const ACK_PUBLISH_FEED: &str = "INSERT INTO feed_items(item_id,space_id,item_type,resource_id,resource_version,producer_id,thread_id,committed_at) SELECT ai.candidate_feed_id,ai.space_id,'receipt',r.receipt_id,r.version,?,f.thread_id,r.updated_at FROM submission_ack_items ai JOIN receipts r ON r.space_id=ai.space_id AND r.consumer_id=(SELECT resource_id FROM submissions WHERE submission_id=ai.submission_id) AND r.subject_item_id=ai.subject_item_id JOIN feed_items f ON f.space_id=ai.space_id AND f.item_id=ai.subject_item_id WHERE ai.submission_id=? AND r.last_submission_id=? AND f.item_type<>'receipt' AND NOT EXISTS(SELECT 1 FROM feed_receipt_updates u WHERE u.space_id=r.space_id AND u.receipt_id=r.receipt_id AND u.receipt_version=r.version) AND {guard}";

pub const ACK_SNAPSHOT_RECEIPTS: &str = "INSERT INTO feed_receipt_updates(space_id,item_id,receipt_id,subject_item_id,subject_resource_id,consumer_id,actor_id,stage,receipt_version,updated_at) SELECT ai.space_id,ai.candidate_feed_id,r.receipt_id,ai.subject_item_id,old.resource_id,r.consumer_id,?,r.stage,r.version,r.updated_at FROM submission_ack_items ai JOIN receipts r ON r.space_id=ai.space_id AND r.consumer_id=(SELECT resource_id FROM submissions WHERE submission_id=ai.submission_id) AND r.subject_item_id=ai.subject_item_id JOIN feed_items new ON new.space_id=ai.space_id AND new.item_id=ai.candidate_feed_id JOIN feed_items old ON old.space_id=ai.space_id AND old.item_id=ai.subject_item_id WHERE ai.submission_id=? AND {guard}";

pub const ACK_DELIVER_RECEIPTS: &str = "INSERT INTO feed_recipients(space_id,item_id,actor_id) SELECT ai.space_id,ai.candidate_feed_id,old.producer_id FROM submission_ack_items ai JOIN feed_items new ON new.space_id=ai.space_id AND new.item_id=ai.candidate_feed_id JOIN feed_items old ON old.space_id=ai.space_id AND old.item_id=ai.subject_item_id WHERE ai.submission_id=? AND {guard}";

pub const ACK_SAVE_RESULTS: &str = "UPDATE submission_ack_items SET result_stage=(SELECT r.stage FROM receipts r WHERE r.space_id=submission_ack_items.space_id AND r.consumer_id=(SELECT resource_id FROM submissions WHERE submission_id=submission_ack_items.submission_id) AND r.subject_item_id=submission_ack_items.subject_item_id),result_version=(SELECT r.version FROM receipts r WHERE r.space_id=submission_ack_items.space_id AND r.consumer_id=(SELECT resource_id FROM submissions WHERE submission_id=submission_ack_items.submission_id) AND r.subject_item_id=submission_ack_items.subject_item_id) WHERE submission_id=? AND {guard}";

pub const ACK_CHECKPOINT: &str = "INSERT INTO consumer_checkpoints(space_id,consumer_id,view_revision,grant_fingerprint,sequence,updated_at) SELECT ?,?,?,?,?,? WHERE {guard} ON CONFLICT(space_id,consumer_id,view_revision,grant_fingerprint) DO UPDATE SET sequence=excluded.sequence,updated_at=excluded.updated_at WHERE excluded.sequence>consumer_checkpoints.sequence";
