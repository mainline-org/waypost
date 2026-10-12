use super::*;
use crate::json::fields;

// Committed blobs (including unshared uploads and historical manifests) are retained.
// Only terminal submissions' unreferenced objects and expired pending uploads qualify.
const UNREFERENCED: &str = "NOT EXISTS(SELECT 1 FROM messages m WHERE m.body_ref=w.object_key)
 AND NOT EXISTS(SELECT 1 FROM blobs b WHERE b.object_key=w.object_key)
 AND NOT EXISTS(SELECT 1 FROM uploads u WHERE u.object_key=w.object_key AND (u.state='sealed' OR u.expires_at>?))";
const RETRY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
const RESWEEP_MS: i64 = 60 * 60 * 1000;

impl<D: Database, O: ObjectStore> App<D, O> {
    pub(super) async fn journal_object(
        &self,
        p: &Principal,
        submission: &str,
        key: &str,
    ) -> Result<()> {
        let (authority, auth) = self.authority_guard(p);
        let mut args = vec![
            key.into(),
            submission.into(),
            self.clock.now_ms().into(),
            submission.into(),
        ];
        args.extend(auth);
        self.db.batch(vec![Statement::new(format!("INSERT INTO object_writes(object_key,submission_id,created_at) SELECT ?,?,? WHERE EXISTS(SELECT 1 FROM submissions WHERE submission_id=? AND state IN('reserved','prepared')) AND {authority} ON CONFLICT(object_key) DO NOTHING"),args)]).await?;
        let rows=self.db.query(Statement::new("SELECT 1 AS writable FROM object_writes w JOIN submissions s ON s.submission_id=w.submission_id WHERE w.object_key=? AND w.submission_id=? AND w.state='active' AND s.state IN('reserved','prepared')",vec![key.into(),submission.into()])).await?;
        if rows.is_empty() {
            return Err(Error::new(
                "SUBMISSION_PENDING",
                "Object preparation is no longer writable; check submission status.",
            ));
        }
        Ok(())
    }
    pub(crate) async fn admin_cleanup(&self, body: Value) -> Result<Value> {
        fields(&body, &["dry_run", "limit", "cursor"])?;
        let dry_run = body
            .get("dry_run")
            .and_then(Value::as_bool)
            .ok_or_else(|| Error::invalid("dry_run must be a boolean."))?;
        let limit = super::mailbox::page_limit(&body, self.limits.max_page_size.min(8))?;
        let now = self.clock.now_ms();
        let mut after = String::new();
        if let Some(cursor) = body.get("cursor").filter(|v| !v.is_null()) {
            let payload = self.verify_cursor(
                cursor
                    .as_str()
                    .ok_or_else(|| Error::invalid("Invalid cleanup cursor."))?,
            )?;
            after = payload
                .strip_prefix("gc1|")
                .ok_or_else(|| Error::invalid("Invalid cleanup cursor."))?
                .to_owned();
            crate::domain::check_id(&after)?;
        }
        let rows=self.db.query(Statement::new(format!("SELECT w.object_key,w.submission_id FROM object_writes w JOIN submissions s ON s.submission_id=w.submission_id WHERE w.object_key>? AND (w.lease_until IS NULL OR w.lease_until<=?) AND (w.last_swept_at IS NULL OR w.last_swept_at<=?) AND {UNREFERENCED} AND (s.state IN('failed','committed') OR s.created_at<=?) ORDER BY w.object_key LIMIT ?"),vec![after.into(),now.into(),(now-RESWEEP_MS).into(),now.into(),(now-RETRY_WINDOW_MS).into(),((limit+1) as i64).into()])).await?;
        let more = rows.len() > limit;
        let candidates = rows
            .iter()
            .take(limit)
            .map(|r| text(r, "object_key"))
            .collect::<Result<Vec<_>>>()?;
        let next = candidates
            .last()
            .map(|key| self.sign_cursor(&format!("gc1|{key}")))
            .transpose()?;
        if dry_run {
            return Ok(
                json!({"dry_run":true,"candidate_count":candidates.len(),"has_more":more,"next_cursor":next}),
            );
        }
        let mut removed = 0;
        for row in rows.iter().take(limit) {
            let key = text(row, "object_key")?;
            let sid = text(row, "submission_id")?;
            let lease = random_id("cleanup")?;
            let q = vec![
                Statement::new(
                    "UPDATE submissions SET state='failed',error_code='SUBMISSION_EXPIRED' WHERE submission_id=? AND state IN('reserved','prepared') AND created_at<=?",
                    vec![sid.into(), (now - RETRY_WINDOW_MS).into()],
                ),
                Statement::new(
                    format!(
                        "UPDATE object_writes AS w SET state='cleaning',lease_nonce=?,lease_until=? WHERE object_key=? AND (lease_until IS NULL OR lease_until<=?) AND {UNREFERENCED} AND EXISTS(SELECT 1 FROM submissions s WHERE s.submission_id=w.submission_id AND s.state IN('failed','committed'))"
                    ),
                    vec![
                        lease.clone().into(),
                        (now + 300_000).into(),
                        key.clone().into(),
                        now.into(),
                        now.into(),
                    ],
                ),
            ];
            self.db.batch(q).await?;
            let claimed=self.db.query(Statement::new("SELECT 1 AS claimed FROM object_writes WHERE object_key=? AND lease_nonce=? AND state='cleaning'",vec![key.clone().into(),lease.clone().into()])).await?;
            if claimed.is_empty() {
                continue;
            }
            // No terminal submission can commit again. Expired uploads cannot seal.
            self.objects.remove(&key).await?;
            self.db.batch(vec![Statement::new("UPDATE object_writes SET last_swept_at=?,lease_nonce=NULL,lease_until=NULL WHERE object_key=? AND lease_nonce=?",vec![self.clock.now_ms().into(),key.into(),lease.into()])]).await?;
            removed += 1;
        }
        Ok(
            json!({"dry_run":false,"candidate_count":candidates.len(),"removed":removed,"has_more":more,"next_cursor":next}),
        )
    }
}
