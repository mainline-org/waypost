use super::*;
use crate::{
    domain::{ReceiptStage, check_request_id},
    json::{fields, required},
    storage::driver::optional_text,
};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AckItem {
    item_id: String,
    stage: ReceiptStage,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Ack {
    request_id: String,
    consumer_id: String,
    items: Vec<AckItem>,
    #[serde(default)]
    checkpoint_cursor: Option<String>,
}
impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) async fn ack(&self, p: &Principal, value: Value) -> Result<Value> {
        p.require("receipt.write")?;
        let fp = fingerprint(&value)?;
        let input: Ack = crate::json::decode(value)?;
        check_request_id(&input.request_id)?;
        if input.consumer_id != p.consumer_id {
            return Err(Error::not_found());
        }
        if input.items.len() > 20 {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Too many acknowledgment items.",
            ));
        }
        if let Some(row) = self.submission(p, "receipt.ack", &input.request_id).await? {
            self.existing_result(&row, &fp)?;
            if text(&row, "state")? == "committed" {
                self.current(p).await?;
                return self.ack_result(&row).await;
            }
        }
        let checkpoint = match &input.checkpoint_cursor {
            Some(c) => Some(self.feed_position(p, c)?),
            None => None,
        };
        let mut desired: BTreeMap<String, i64> = BTreeMap::new();
        for item in input.items {
            crate::domain::check_id(&item.item_id)?;
            let entry = desired.entry(item.item_id).or_default();
            *entry = (*entry).max(item.stage.rank());
        }
        let ids = desired.keys().cloned().collect::<Vec<_>>();
        let visible = self
            .read_feed_rows(p, 0, ids.len().max(1), Some(&ids))
            .await?;
        if visible.len() != ids.len() {
            return Err(Error::not_found());
        }
        let row = self
            .reserve(
                p,
                "receipt.ack",
                &input.request_id,
                &fp,
                &p.consumer_id,
                None,
                &[],
            )
            .await?;
        if text(&row, "state")? == "committed" {
            return self.ack_result(&row).await;
        }
        let sid = text(&row, "submission_id")?;
        let nonce = random_id("attempt")?;
        let now = self.clock.now_ms();
        let (authority, auth_args) = self.authority_guard(p);
        let visibility = self.feed_visibility(p, "f");
        let mut args = vec![nonce.clone().into(), sid.clone().into()];
        args.extend(auth_args);
        let mut condition = format!(
            "UPDATE submissions SET attempt_nonce=?,state='prepared' WHERE submission_id=? AND state IN('reserved','prepared') AND {authority}"
        );
        if !ids.is_empty() {
            condition.push_str(&format!(" AND (SELECT COUNT(*) FROM feed_items f WHERE f.space_id=? AND f.item_id IN ({}) AND {})=?",vec!["?";ids.len()].join(","),visibility.0));
            args.push(p.space_id.clone().into());
            args.extend(ids.iter().cloned().map(Sql::from));
            args.extend(visibility.1);
            args.push((ids.len() as i64).into());
        }
        let guard=("EXISTS(SELECT 1 FROM submissions WHERE submission_id=? AND attempt_nonce=? AND state='prepared')".into(),vec![sid.clone().into(),nonce.clone().into()]);
        let mut q = vec![Statement::new(condition, args)];
        let mut staging = Vec::new();
        for (position, (item, stage)) in desired.iter().enumerate() {
            staging.push(vec![
                sid.clone().into(),
                p.space_id.clone().into(),
                (position as i64).into(),
                item.clone().into(),
                (*stage).into(),
                random_id("receipt")?.into(),
                random_id("feed")?.into(),
            ]);
        }
        q.extend(push_rows("submission_ack_items","submission_id,space_id,position,subject_item_id,desired_stage,candidate_receipt_id,candidate_feed_id",staging,Some(guard.clone())));
        let mut args = vec![
            p.consumer_id.clone().into(),
            now.into(),
            sid.clone().into(),
            sid.clone().into(),
        ];
        args.extend(guard.1.clone());
        q.push(Statement::new(
            crate::storage::queries::ACK_CREATE_RECEIPTS.replace("{guard}", &guard.0),
            args,
        ));
        let mut args = vec![
            sid.clone().into(),
            now.into(),
            sid.clone().into(),
            p.space_id.clone().into(),
            p.consumer_id.clone().into(),
            sid.clone().into(),
        ];
        args.extend(guard.1.clone());
        q.push(Statement::new(
            crate::storage::queries::ACK_ADVANCE_RECEIPTS.replace("{guard}", &guard.0),
            args,
        ));
        let mut args = vec![
            p.actor_id.clone().into(),
            sid.clone().into(),
            sid.clone().into(),
        ];
        args.extend(guard.1.clone());
        q.push(Statement::new(
            crate::storage::queries::ACK_PUBLISH_FEED.replace("{guard}", &guard.0),
            args,
        ));
        let mut args = vec![p.actor_id.clone().into(), sid.clone().into()];
        args.extend(guard.1.clone());
        q.push(Statement::new(
            crate::storage::queries::ACK_SNAPSHOT_RECEIPTS.replace("{guard}", &guard.0),
            args,
        ));
        let mut args = vec![sid.clone().into()];
        args.extend(guard.1.clone());
        q.push(Statement::new(
            crate::storage::queries::ACK_DELIVER_RECEIPTS.replace("{guard}", &guard.0),
            args,
        ));
        let mut args = vec![sid.clone().into()];
        args.extend(guard.1.clone());
        q.push(Statement::new(
            crate::storage::queries::ACK_SAVE_RESULTS.replace("{guard}", &guard.0),
            args,
        ));
        if let Some(position) = checkpoint {
            let mut args = vec![
                p.space_id.clone().into(),
                p.consumer_id.clone().into(),
                p.view_revision.into(),
                p.grant_fingerprint.clone().into(),
                (position as i64).into(),
                now.into(),
            ];
            args.extend(guard.1.clone());
            q.push(Statement::new(
                crate::storage::queries::ACK_CHECKPOINT.replace("{guard}", &guard.0),
                args,
            ));
        }
        q.push(Statement::new("UPDATE submissions SET state='committed',committed_at=?,result_checkpoint_cursor=? WHERE submission_id=? AND attempt_nonce=? AND state='prepared'",vec![now.into(),input.checkpoint_cursor.into(),sid.into(),nonce.into()]));
        self.db.batch(q).await?;
        self.current(p).await?;
        let result = self
            .submission(p, "receipt.ack", &input.request_id)
            .await?
            .ok_or_else(Error::storage)?;
        if text(&result, "state")? != "committed" {
            return Err(Error::new(
                "SUBMISSION_PENDING",
                "Acknowledgment is not committed yet.",
            ));
        }
        self.ack_result(&result).await
    }
    pub(crate) async fn ack_result(&self, row: &Row) -> Result<Value> {
        let rows=self.db.query(Statement::new("SELECT subject_item_id,result_stage,result_version FROM submission_ack_items WHERE submission_id=? ORDER BY position",vec![text(row,"submission_id")?.into()])).await?;
        let mut items = Vec::new();
        for r in rows {
            let rank = integer(&r, "result_stage")?;
            items.push(json!({"item_id":text(&r,"subject_item_id")?,"stage":if rank==2{"processed"}else{"received"},"version":integer(&r,"result_version")?}));
        }
        Ok(
            json!({"submission_state":"committed","consumer_id":text(row,"resource_id")?,"items":items,"checkpoint_cursor":optional_text(row,"result_checkpoint_cursor")}),
        )
    }
    pub(crate) fn feed_visibility(&self, p: &Principal, alias: &str) -> (String, Vec<Sql>) {
        let observe = p.view == "space_log" && p.has("space.observe");
        (
            format!(
                "(({}.item_type='message' AND ?=1 AND (?=1 OR EXISTS(SELECT 1 FROM feed_recipients vr WHERE vr.space_id={alias}.space_id AND vr.item_id={alias}.item_id AND vr.actor_id=?))) OR ({}.item_type='receipt' AND (?=1 OR EXISTS(SELECT 1 FROM feed_recipients vr WHERE vr.space_id={alias}.space_id AND vr.item_id={alias}.item_id AND vr.actor_id=?))) OR ({alias}.item_type='memory_revision' AND ?=1 AND EXISTS(SELECT 1 FROM memory_collections mc WHERE mc.space_id={alias}.space_id AND mc.collection_id={alias}.resource_id AND (mc.source_actor_id=? OR EXISTS(SELECT 1 FROM token_collections tc WHERE tc.token_id=? AND tc.space_id=mc.space_id AND tc.collection_id=mc.collection_id AND tc.action='read')))))",
                alias, alias
            ),
            vec![
                (observe || p.has("message.read")).into(),
                observe.into(),
                p.actor_id.clone().into(),
                observe.into(),
                p.actor_id.clone().into(),
                p.has("memory.read").into(),
                p.actor_id.clone().into(),
                p.token_id.clone().into(),
            ],
        )
    }
    pub(crate) async fn read_feed_rows(
        &self,
        p: &Principal,
        position: u64,
        limit: usize,
        ids: Option<&[String]>,
    ) -> Result<Vec<Row>> {
        if ids.is_some_and(|i| i.is_empty()) {
            return Ok(vec![]);
        }
        let (authority, auth) = self.authority_guard(p);
        let (visible, visibility) = self.feed_visibility(p, "f");
        let mut sql = format!(
            "SELECT f.*,CAST(f.space_sequence AS TEXT) AS visible_seq,m.subject,m.kind,mr.parent_revision,mr.manifest_blob_id,u.subject_item_id,u.subject_resource_id,u.consumer_id AS receipt_consumer_id,u.actor_id AS receipt_actor_id,u.stage AS receipt_stage,u.receipt_version,u.updated_at AS receipt_updated_at FROM feed_items f LEFT JOIN messages m ON f.item_type='message' AND m.space_id=f.space_id AND m.message_id=f.resource_id LEFT JOIN memory_revisions mr ON f.item_type='memory_revision' AND mr.space_id=f.space_id AND mr.collection_id=f.resource_id AND mr.revision=f.resource_version LEFT JOIN feed_receipt_updates u ON f.item_type='receipt' AND u.space_id=f.space_id AND u.item_id=f.item_id WHERE f.space_id=? AND f.space_sequence>CAST(? AS INTEGER) AND {authority} AND {visible}"
        );
        let mut args = vec![p.space_id.clone().into(), position.to_string().into()];
        args.extend(auth);
        args.extend(visibility);
        if let Some(ids) = ids {
            sql.push_str(&format!(
                " AND f.item_id IN ({})",
                vec!["?"; ids.len()].join(",")
            ));
            args.extend(ids.iter().cloned().map(Sql::from));
        }
        for (column, values) in [
            ("f.producer_id", &p.filter.producer_ids),
            ("f.thread_id", &p.filter.thread_ids),
            ("f.item_type", &p.filter.item_types),
            ("f.resource_id", &p.filter.collection_ids),
        ] {
            if !values.is_empty() {
                sql.push_str(&format!(
                    " AND {column} IN ({})",
                    vec!["?"; values.len()].join(",")
                ));
                args.extend(values.iter().cloned().map(Sql::from));
            }
        }
        sql.push_str(" ORDER BY f.space_sequence LIMIT ?");
        args.push((limit as i64).into());
        self.db.query(Statement::new(sql, args)).await
    }
    pub(crate) async fn feed_page(&self, p: &Principal, value: Value) -> Result<Value> {
        fields(&value, &["consumer_id", "cursor", "limit"])?;
        if required(&value, "consumer_id")? != p.consumer_id {
            return Err(Error::not_found());
        }
        if p.view == "space_log" {
            p.require("space.observe")?;
        } else if !p.has("message.read") && !p.has("memory.read") {
            return Err(Error::forbidden());
        }
        let position = match value.get("cursor") {
            None | Some(Value::Null) => 0,
            Some(Value::String(s)) => self.feed_position(p, s)?,
            _ => return Err(Error::invalid("Invalid cursor.")),
        };
        let limit = super::mailbox::page_limit(&value, self.limits.max_page_size)?;
        let mut rows = self.read_feed_rows(p, position, limit + 1, None).await?;
        let more = rows.len() > limit;
        if more {
            rows.truncate(limit);
        }
        let recipients = self.feed_recipients(p, &rows).await?;
        let mut items = Vec::new();
        let mut next = position;
        for row in rows {
            let feed = crate::storage::rows::FeedItem::decode(&row)?;
            let item = feed.id;
            next = feed.position;
            let kind = text(&row, "item_type")?;
            let data = if matches!(feed.kind, crate::storage::rows::FeedKind::Message) {
                json!({"message_id":text(&row,"resource_id")?,"subject":self.open_field(&p.space_id,&format!("message/{}/subject",feed.resource_id),&text(&row,"subject")?)?,"kind":text(&row,"kind")?})
            } else if matches!(feed.kind, crate::storage::rows::FeedKind::MemoryRevision) {
                json!({"collection_id":text(&row,"resource_id")?,"revision":integer(&row,"resource_version")?,"parent_revision":integer(&row,"parent_revision")?,"manifest_blob_id":text(&row,"manifest_blob_id")?})
            } else {
                json!({"receipt_id":text(&row,"resource_id")?,"subject_item_id":text(&row,"subject_item_id")?,"subject_resource_id":text(&row,"subject_resource_id")?,"consumer_id":text(&row,"receipt_consumer_id")?,"actor_id":text(&row,"receipt_actor_id")?,"stage":if integer(&row,"receipt_stage")?==2{"processed"}else{"received"},"updated_at":stamp(integer(&row,"receipt_updated_at")?)?})
            };
            items.push(json!({"item_id":item,"space_id":p.space_id,"visible_seq":next.to_string(),"item_type":kind,"resource_id":feed.resource_id,"resource_version":feed.version,"producer_id":feed.producer_id,"recipient_ids":recipients.get(&item).cloned().unwrap_or_default(),"thread_id":feed.thread_id,"committed_at":stamp(feed.committed_at)?,"data":data}));
        }
        self.current(p).await?;
        Ok(json!({"items":items,"next_cursor":self.feed_cursor(p,next)?,"has_more":more}))
    }
}
