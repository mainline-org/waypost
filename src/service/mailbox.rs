use super::*;
use crate::{
    domain::{Send, check_id, check_request_id},
    json::{fields, required},
    storage::driver::optional_text,
};

impl<D: Database, O: ObjectStore> App<D, O> {
    pub(crate) async fn submission(
        &self,
        p: &Principal,
        operation: &str,
        key: &str,
    ) -> Result<Option<Row>> {
        let rows=self.db.query(Statement::new("SELECT * FROM submissions WHERE space_id=? AND producer_id=? AND operation=? AND request_id=?",vec![p.space_id.clone().into(),p.actor_id.clone().into(),operation.into(),key.into()])).await?;
        Ok(rows.first().cloned())
    }
    pub(crate) fn existing_result(&self, row: &Row, fp: &str) -> Result<Option<Value>> {
        if hex::encode(blob(row, "fingerprint")?) != fp {
            return Err(Error::new(
                "IDEMPOTENCY_CONFLICT",
                "Request ID was used with different content.",
            ));
        }
        match text(row, "state")?.as_str() {
            "committed" => Ok(Some(self.committed_result(row)?)),
            "failed" => Err(recorded_error(row)?),
            _ => Ok(None),
        }
    }
    pub(crate) fn committed_result(&self, row: &Row) -> Result<Value> {
        let op = text(row, "operation")?;
        let resource = text(row, "resource_id")?;
        match op.as_str() {
            "message.send" => Ok(
                json!({"submission_state":"committed","resource_id":resource,"message_id":resource,"thread_id":text(row,"thread_id")?,"committed_at":stamp(integer(row,"committed_at")?)?}),
            ),
            "memory.publish" => Ok(
                json!({"submission_state":"committed","collection_id":resource,"revision":integer(row,"result_revision")?,"changed":integer(row,"result_changed")?==1,"manifest_blob_id":optional_text(row,"result_manifest_blob_id")}),
            ),
            _ => Ok(
                json!({"submission_state":"committed","resource_id":resource,"committed_at":stamp(integer(row,"committed_at")?)?}),
            ),
        }
    }
    // Reservation lists its durable identity fields explicitly; no hidden builder state.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn reserve(
        &self,
        p: &Principal,
        operation: &str,
        key: &str,
        fp: &str,
        resource: &str,
        thread: Option<String>,
        recipients: &[String],
    ) -> Result<Row> {
        let id = random_id("submission")?;
        let (authority, mut args) = self.authority_guard(p);
        let mut parameters = vec![
            id.clone().into(),
            p.space_id.clone().into(),
            p.actor_id.clone().into(),
            operation.into(),
            key.into(),
            hex::decode(fp).map_err(|_| Error::storage())?.into(),
            resource.into(),
            thread.into(),
            self.clock.now_ms().into(),
        ];
        parameters.append(&mut args);
        let sql = format!(
            "INSERT OR IGNORE INTO submissions(submission_id,space_id,producer_id,operation,request_id,fingerprint,resource_id,thread_id,state,created_at) SELECT ?,?,?,?,?,?,?,?,'reserved',? WHERE {authority}"
        );
        let guard = (
            "EXISTS(SELECT 1 FROM submissions WHERE submission_id=? AND state='reserved')".into(),
            vec![id.clone().into()],
        );
        let mut q = vec![Statement::new(sql, parameters)];
        q.extend(push_rows(
            "submission_recipients",
            "submission_id,space_id,actor_id",
            recipients
                .iter()
                .map(|r| {
                    vec![
                        id.clone().into(),
                        p.space_id.clone().into(),
                        r.clone().into(),
                    ]
                })
                .collect(),
            Some(guard),
        ));
        self.db.batch(q).await?;
        let row = self
            .submission(p, operation, key)
            .await?
            .ok_or_else(Error::unauthenticated)?;
        self.existing_result(&row, fp)?;
        Ok(row)
    }
    pub(crate) async fn prepared(
        &self,
        p: &Principal,
        row: &Row,
        key: &str,
        bytes: &[u8],
        content_type: &str,
    ) -> Result<Row> {
        let id = text(row, "submission_id")?;
        let expected = Sha256::digest(bytes).to_vec();
        let object = self
            .objects
            .create_only(key, bytes, content_type, &expected)
            .await?;
        let (authority, args) = self.authority_guard(p);
        let mut parameters = vec![
            key.into(),
            object.sha256.into(),
            (object.size as i64).into(),
            self.clock.now_ms().into(),
            id.clone().into(),
        ];
        parameters.extend(args);
        self.db.batch(vec![Statement::new(format!("UPDATE submissions SET state='prepared',body_ref=?,body_sha256=?,body_size=?,prepared_at=? WHERE submission_id=? AND state IN('reserved','prepared') AND {authority}"),parameters)]).await?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM submissions WHERE submission_id=?",
                vec![id.into()],
            ))
            .await?;
        let row = rows.first().cloned().ok_or_else(Error::storage)?;
        self.current(p).await?;
        Ok(row)
    }
    pub(crate) async fn fail_submission(
        &self,
        p: &Principal,
        row: &Row,
        error: Error,
        policy_revision: Option<i64>,
    ) -> Result<Value> {
        if !matches!(
            error.code,
            "INVALID_ARGUMENT" | "RESOURCE_LIMIT" | "PRIVACY_BLOCKED"
        ) {
            return Err(error);
        }
        let (mut authority, args) = self.authority_guard(p);
        let mut parameters = vec![error.code.into(), text(row, "submission_id")?.into()];
        parameters.extend(args);
        if let Some(revision) = policy_revision {
            authority.push_str(
                " AND EXISTS(SELECT 1 FROM spaces WHERE space_id=? AND privacy_revision=?)",
            );
            parameters.extend([p.space_id.clone().into(), revision.into()]);
        }
        self.db.batch(vec![Statement::new(format!("UPDATE submissions SET state='failed',error_code=? WHERE submission_id=? AND state IN('reserved','prepared') AND {authority}"),parameters)]).await?;
        self.current(p).await?;
        let rows = self
            .db
            .query(Statement::new(
                "SELECT * FROM submissions WHERE submission_id=?",
                vec![text(row, "submission_id")?.into()],
            ))
            .await?;
        let row = rows.first().ok_or_else(Error::storage)?;
        if text(row, "state")? == "committed" {
            return self.committed_result(row);
        }
        if text(row, "state")? == "failed" {
            return Err(recorded_error(row)?);
        }
        Err(Error::new(
            "SUBMISSION_PENDING",
            "Submission outcome is pending.",
        ))
    }
    pub(crate) async fn send(&self, p: &Principal, value: Value) -> Result<Value> {
        p.require("message.send")?;
        let mut input: Send = crate::json::decode(value)?;
        input.validate()?;
        if input.attachment_ids.len() > self.limits.max_message_attachments {
            return Err(Error::new(
                "RESOURCE_LIMIT",
                "Too many attachments for this deployment.",
            ));
        }
        let mut recipients = input
            .recipient_ids
            .clone()
            .unwrap_or_else(|| vec![p.default_actor_id.clone()]);
        recipients.sort();
        input.recipient_ids = Some(recipients.clone());
        let allowed = if p.grants.recipient_ids.is_empty() {
            vec![p.default_actor_id.clone()]
        } else {
            p.grants.recipient_ids.clone()
        };
        if recipients.iter().any(|id| !allowed.contains(id)) {
            return Err(Error::forbidden());
        }
        let previous = self
            .submission(p, "message.send", &input.request_id)
            .await?;
        let mut new_thread = input.thread_id.is_none() && input.in_reply_to.is_none();
        if let Some(original) = &input.in_reply_to {
            if let Some(row) = previous
                .as_ref()
                .filter(|r| matches!(text(r, "state").as_deref(), Ok("committed" | "failed")))
            {
                // Recover the original normalized thread before revalidating mutable reply-read grants.
                if input.thread_id.is_none() {
                    input.thread_id = Some(text(row, "thread_id")?);
                }
            } else {
                let original = self.message(p, original).await?;
                let thread = original["thread_id"].as_str().ok_or_else(Error::storage)?;
                if input.thread_id.as_ref().is_some_and(|id| id != thread) {
                    return Err(Error::invalid("Reply belongs to another thread."));
                }
                input.thread_id = Some(thread.into());
            }
            new_thread = false;
        }
        let normalized = serde_json::to_value(&input).map_err(|_| Error::storage())?;
        let fp = fingerprint(&normalized)?;
        if let Some(row) = previous
            && let Some(result) = self.existing_result(&row, &fp)?
        {
            self.current(p).await?;
            return Ok(result);
        }
        for id in &input.attachment_ids {
            self.reference_blob(p, id).await?;
        }
        let placeholders = vec!["?"; recipients.len()].join(",");
        let mut args = vec![p.space_id.clone().into()];
        args.extend(recipients.iter().cloned().map(Sql::from));
        let actors=self.db.query(Statement::new(format!("SELECT actor_id FROM actors WHERE space_id=? AND disabled=0 AND actor_id IN ({placeholders})"),args)).await?;
        if actors.len() != recipients.len() {
            return Err(Error::not_found());
        }
        if !new_thread {
            let tid = input
                .thread_id
                .as_ref()
                .ok_or_else(|| Error::invalid("Missing thread."))?;
            let mut members = recipients.clone();
            members.push(p.actor_id.clone());
            members.sort();
            members.dedup();
            let mut args = vec![p.space_id.clone().into(), tid.clone().into()];
            args.extend(members.iter().cloned().map(Sql::from));
            let rows=self.db.query(Statement::new(format!("SELECT actor_id FROM thread_members WHERE space_id=? AND thread_id=? AND actor_id IN ({})",vec!["?";members.len()].join(",")),args)).await?;
            if rows.len() != members.len() {
                return Err(Error::forbidden());
            }
        }
        let proposed_id = random_id("message")?;
        let proposed_thread = if new_thread {
            random_id("thread")?
        } else {
            input.thread_id.clone().ok_or_else(Error::storage)?
        };
        let row = self
            .reserve(
                p,
                "message.send",
                &input.request_id,
                &fp,
                &proposed_id,
                Some(proposed_thread),
                &recipients,
            )
            .await?;
        if let Some(result) = self.existing_result(&row, &fp)? {
            return Ok(result);
        }
        let message_id = text(&row, "resource_id")?;
        let tid = text(&row, "thread_id")?;
        let policy = self.policy(&p.space_id).await?;
        let policy_revision = policy.revision;
        let filtered = match self.filter_message(p, &row, input, policy).await {
            Ok(filtered) => filtered,
            Err(error) => {
                return self
                    .fail_submission(p, &row, error, Some(policy_revision))
                    .await;
            }
        };
        let input = filtered.input;
        let body_key = format!("body_{message_id}_p{}", filtered.revision);
        let mut published = serde_json::to_value(&input).map_err(|_| Error::storage())?;
        published
            .as_object_mut()
            .ok_or_else(Error::storage)?
            .remove("request_id");
        let bytes = crate::json::canonical(&published)?;
        if bytes.len() > self.limits.max_json_bytes {
            return self
                .fail_submission(
                    p,
                    &row,
                    Error::new(
                        "RESOURCE_LIMIT",
                        "Filtered message exceeds deployment limit.",
                    ),
                    Some(policy_revision),
                )
                .await;
        }
        let row = self
            .prepared(p, &row, &body_key, &bytes, "application/json")
            .await?;
        if let Some(result) = self.existing_result(&row, &fp)? {
            return Ok(result);
        }
        let sid = text(&row, "submission_id")?;
        let nonce = random_id("attempt")?;
        let now = self.clock.now_ms();
        let feed = random_id("feed")?;
        let (authority, auth_args) = self.authority_guard(p);
        let mut guard_args = vec![nonce.clone().into(), sid.clone().into()];
        guard_args.extend(auth_args);
        let mut guard_sql = format!(
            "UPDATE submissions SET attempt_nonce=? WHERE submission_id=? AND state='prepared' AND {authority} AND NOT EXISTS(SELECT 1 FROM submission_recipients sr JOIN actors a ON a.space_id=sr.space_id AND a.actor_id=sr.actor_id WHERE sr.submission_id=? AND a.disabled=1)"
        );
        guard_args.push(sid.clone().into());
        guard_sql.push_str(" AND body_ref=? AND EXISTS(SELECT 1 FROM spaces WHERE space_id=? AND privacy_revision=?)");
        guard_args.extend(vec![
            body_key.clone().into(),
            p.space_id.clone().into(),
            filtered.revision.into(),
        ]);
        if !new_thread {
            guard_sql.push_str(" AND EXISTS(SELECT 1 FROM thread_members WHERE space_id=? AND thread_id=? AND actor_id=?) AND NOT EXISTS(SELECT 1 FROM submission_recipients sr WHERE sr.submission_id=? AND NOT EXISTS(SELECT 1 FROM thread_members tm WHERE tm.space_id=sr.space_id AND tm.thread_id=? AND tm.actor_id=sr.actor_id))");
            guard_args.extend(vec![
                p.space_id.clone().into(),
                tid.clone().into(),
                p.actor_id.clone().into(),
                sid.clone().into(),
                tid.clone().into(),
            ]);
        }
        let guard=("EXISTS(SELECT 1 FROM submissions WHERE submission_id=? AND attempt_nonce=? AND state='prepared')".into(),vec![sid.clone().into(),nonce.clone().into()]);
        let mut q = vec![Statement::new(guard_sql, guard_args)];
        if new_thread {
            q.extend(push_rows(
                "threads",
                "space_id,thread_id,created_at",
                vec![vec![
                    p.space_id.clone().into(),
                    tid.clone().into(),
                    now.into(),
                ]],
                Some(guard.clone()),
            ));
            let mut members = recipients.clone();
            members.push(p.actor_id.clone());
            members.sort();
            members.dedup();
            q.extend(push_rows(
                "thread_members",
                "space_id,thread_id,actor_id",
                members
                    .into_iter()
                    .map(|id| vec![p.space_id.clone().into(), tid.clone().into(), id.into()])
                    .collect(),
                Some(guard.clone()),
            ));
        }
        q.extend(push_rows("blobs","space_id,blob_id,owner_actor_id,object_key,content_type,size_bytes,sha256,filename,sealed_at",filtered.blobs,Some(guard.clone())));
        q.extend(push_rows(
            "privacy_audit",
            "space_id,submission_id,privacy_revision,action,rule_id,field,count,created_at",
            filtered.hits,
            Some(guard.clone()),
        ));
        q.extend(push_rows("messages","space_id,message_id,submission_id,producer_id,thread_id,in_reply_to,kind,subject,body_ref,committed_at",vec![vec![p.space_id.clone().into(),message_id.clone().into(),sid.clone().into(),p.actor_id.clone().into(),tid.clone().into(),input.in_reply_to.into(),input.kind.as_str().into(),input.subject.into(),body_key.into(),now.into()]],Some(guard.clone())));
        q.extend(push_rows(
            "message_recipients",
            "space_id,message_id,actor_id",
            recipients
                .iter()
                .cloned()
                .map(|id| {
                    vec![
                        p.space_id.clone().into(),
                        message_id.clone().into(),
                        id.into(),
                    ]
                })
                .collect(),
            Some(guard.clone()),
        ));
        q.extend(push_rows(
            "message_attachments",
            "space_id,message_id,position,blob_id",
            input
                .attachment_ids
                .iter()
                .enumerate()
                .map(|(i, id)| {
                    vec![
                        p.space_id.clone().into(),
                        message_id.clone().into(),
                        (i as i64).into(),
                        id.clone().into(),
                    ]
                })
                .collect(),
            Some(guard.clone()),
        ));
        q.extend(push_rows("feed_items","item_id,space_id,item_type,resource_id,resource_version,producer_id,thread_id,committed_at",vec![vec![feed.clone().into(),p.space_id.clone().into(),"message".into(),message_id.into(),1i64.into(),p.actor_id.clone().into(),tid.into(),now.into()]],Some(guard.clone())));
        q.extend(push_rows(
            "feed_recipients",
            "space_id,item_id,actor_id",
            recipients
                .into_iter()
                .map(|id| vec![p.space_id.clone().into(), feed.clone().into(), id.into()])
                .collect(),
            Some(guard.clone()),
        ));
        q.push(Statement::new("UPDATE submissions SET state='committed',committed_at=? WHERE submission_id=? AND attempt_nonce=? AND state='prepared'",vec![now.into(),sid.into(),nonce.into()]));
        self.db.batch(q).await?;
        let result = self
            .submission(p, "message.send", &input.request_id)
            .await?
            .ok_or_else(Error::storage)?;
        self.current(p).await?;
        self.existing_result(&result, &fp)?
            .ok_or_else(|| Error::new("SUBMISSION_PENDING", "Submission is not committed yet."))
    }
    pub(crate) async fn message(&self, p: &Principal, id: &str) -> Result<Value> {
        check_id(id)?;
        if !p.has("message.read") && !p.has("space.observe") {
            return Err(Error::forbidden());
        }
        let (authority, auth_args) = self.authority_guard(p);
        let mut args = vec![
            p.space_id.clone().into(),
            id.into(),
            p.has("space.observe").into(),
            p.actor_id.clone().into(),
            p.actor_id.clone().into(),
        ];
        args.extend(auth_args);
        let rows=self.db.query(Statement::new(format!("SELECT m.*,s.body_sha256,s.body_size FROM messages m JOIN submissions s ON s.submission_id=m.submission_id WHERE m.space_id=? AND m.message_id=? AND (?=1 OR m.producer_id=? OR EXISTS(SELECT 1 FROM message_recipients mr WHERE mr.space_id=m.space_id AND mr.message_id=m.message_id AND mr.actor_id=?)) AND {authority}"),args)).await?;
        let row = rows.first().ok_or_else(Error::not_found)?;
        let bytes = self.objects.read(&text(row, "body_ref")?).await?;
        if bytes.len() as i64 != integer(row, "body_size")?
            || Sha256::digest(&bytes).as_slice() != blob(row, "body_sha256")?.as_slice()
        {
            return Err(Error::storage());
        }
        let mut value = crate::json::parse(&bytes, self.limits.max_json_bytes)?;
        let object = value.as_object_mut().ok_or_else(Error::storage)?;
        for (key, v) in [
            ("message_id", json!(text(row, "message_id")?)),
            ("space_id", json!(p.space_id)),
            ("producer_id", json!(text(row, "producer_id")?)),
            ("thread_id", json!(text(row, "thread_id")?)),
            ("committed_at", json!(stamp(integer(row, "committed_at")?)?)),
            ("envelope_version", json!(1)),
        ] {
            object.insert(key.into(), v);
        }
        self.current(p).await?;
        Ok(value)
    }
    pub(crate) fn feed_cursor(&self, p: &Principal, position: u64) -> Result<String> {
        self.sign_cursor(&format!(
            "f1|{}|{}|{}|{}|{}|{}|{}",
            p.space_id,
            p.actor_id,
            p.consumer_id,
            p.view_revision,
            p.filter_fingerprint,
            p.grant_fingerprint,
            position
        ))
    }
    pub(crate) fn feed_position(&self, p: &Principal, cursor: &str) -> Result<u64> {
        let payload = self.verify_cursor(cursor)?;
        let fields = payload.split('|').collect::<Vec<_>>();
        if fields.len() != 8 || fields[0] != "f1" {
            return Err(Error::new("INVALID_CURSOR", "Invalid cursor."));
        }
        if fields[1] != p.space_id
            || fields[2] != p.actor_id
            || fields[3] != p.consumer_id
            || fields[4] != p.view_revision.to_string()
            || fields[5] != p.filter_fingerprint
            || fields[6] != p.grant_fingerprint
        {
            return Err(Error::new(
                "CURSOR_SCOPE_CHANGED",
                "Cursor belongs to another view or grant scope.",
            ));
        }
        let position = fields[7]
            .parse::<u64>()
            .map_err(|_| Error::new("INVALID_CURSOR", "Invalid cursor."))?;
        if position > i64::MAX as u64 {
            return Err(Error::new("INVALID_CURSOR", "Invalid cursor."));
        }
        Ok(position)
    }
    pub(crate) async fn inbox(&self, p: &Principal, value: Value) -> Result<Value> {
        self.feed_page(p, value).await
    }
    pub(crate) async fn feed_recipients(
        &self,
        p: &Principal,
        rows: &[Row],
    ) -> Result<BTreeMap<String, Vec<String>>> {
        let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let ids = rows
            .iter()
            .map(|r| text(r, "item_id"))
            .collect::<Result<Vec<_>>>()?;
        for chunk in ids.chunks(64) {
            let mut args = vec![p.space_id.clone().into()];
            args.extend(chunk.iter().cloned().map(Sql::from));
            let r=self.db.query(Statement::new(format!("SELECT item_id,actor_id FROM feed_recipients WHERE space_id=? AND item_id IN ({}) ORDER BY actor_id",vec!["?";chunk.len()].join(",")),args)).await?;
            for row in r {
                result
                    .entry(text(&row, "item_id")?)
                    .or_default()
                    .push(text(&row, "actor_id")?);
            }
        }
        Ok(result)
    }
    pub(crate) async fn status(&self, p: &Principal, value: Value) -> Result<Value> {
        fields(&value, &["operation", "request_id"])?;
        let operation = required(&value, "operation")?;
        let key = required(&value, "request_id")?;
        check_request_id(&key)?;
        let row = self
            .submission(p, &operation, &key)
            .await?
            .ok_or_else(Error::not_found)?;
        self.current(p).await?;
        let state = text(&row, "state")?;
        Ok(
            json!({"submission_state":state,"submission_id":text(&row,"submission_id")?,"resource_id":text(&row,"resource_id")?,"result":if state=="committed"{Some(if operation=="receipt.ack"{self.ack_result(&row).await?}else if operation=="blob.prepare"{self.upload_result(&row).await?}else if operation=="blob.commit"{self.sealed_result(p,&text(&row,"resource_id")?).await?}else{self.committed_result(&row)?})}else{None},"error":if state=="failed"{Some(recorded_error(&row)?.value())}else{None}}),
        )
    }
}
pub(crate) fn page_limit(v: &Value, max: usize) -> Result<usize> {
    let limit = match v.get("limit") {
        None => 50usize.min(max),
        Some(Value::Number(n)) => n
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .ok_or_else(|| Error::invalid("Invalid page size."))?,
        _ => return Err(Error::invalid("Invalid page size.")),
    };
    if limit == 0 || limit > max {
        return Err(Error::new(
            "RESOURCE_LIMIT",
            "Page size exceeds the deployment limit.",
        ));
    }
    Ok(limit)
}
pub(crate) fn recorded_error(r: &Row) -> Result<Error> {
    let code = optional_text(r, "error_code").ok_or_else(Error::storage)?;
    let known = match code.as_str() {
        "REVISION_CONFLICT" => "REVISION_CONFLICT",
        "INVALID_ARGUMENT" => "INVALID_ARGUMENT",
        "FORBIDDEN" => "FORBIDDEN",
        "BLOB_NOT_SEALED" => "BLOB_NOT_SEALED",
        "UPLOAD_EXPIRED" => "UPLOAD_EXPIRED",
        "RESOURCE_LIMIT" => "RESOURCE_LIMIT",
        "PRIVACY_BLOCKED" => "PRIVACY_BLOCKED",
        _ => return Err(Error::storage()),
    };
    let mut e = Error::new(known, "The original submission failed.");
    if let Ok(n) = integer(r, "error_current_revision") {
        e.details = json!({"current_revision":n});
    }
    Ok(e)
}
