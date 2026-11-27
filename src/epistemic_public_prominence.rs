//! Attributed recognizability reports scoped by audience and two distinct dates.
use super::{WorkbenchError, scoped, situation_visible, valid_text};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Cue {
    Name,
    Face,
    PublicRole,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Observation {
    RecognitionReported,
    NonRecognitionReported,
    ExposureReported,
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub subject_actor_id: Uuid,
    pub audience_actor_id: Uuid,
    pub cue: Cue,
    pub observation: Observation,
    pub audience_scope: String,
    pub effective_from: String,
    pub effective_through: String,
    pub available_on: String,
    pub claim_object_id: Uuid,
    pub representation_id: Uuid,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Lookup {
    pub subject_actor_id: Uuid,
    pub audience_actor_id: Uuid,
    pub cue: Option<Cue>,
    pub on: String,
    pub known_through: String,
}

#[derive(Serialize)]
pub struct Record {
    pub record_id: Uuid,
    #[serde(flatten)]
    pub input: Input,
    pub recorded_by: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Coverage {
    RecordedReports,
    NoRecordedReports,
}

#[derive(Serialize)]
pub struct LookupResult {
    pub query: Lookup,
    coverage: Coverage,
    records: Vec<Record>,
    truncated: bool,
}

const LOOKUP_LIMIT: i64 = 100;
const RECORD_COLUMNS: &str = "
    record_id, subject_actor_id, audience_actor_id, cue, observation, audience_scope,
    to_char(effective_from, 'YYYY-MM-DD') AS effective_from,
    to_char(effective_through, 'YYYY-MM-DD') AS effective_through,
    to_char(available_on, 'YYYY-MM-DD') AS available_on,
    claim_object_id, representation_id, recorded_by";

fn calendar_date(value: &str) -> bool {
    let b = value.as_bytes();
    if b.len() != 10
        || b[4] != b'-'
        || b[7] != b'-'
        || !b[..4]
            .iter()
            .chain(&b[5..7])
            .chain(&b[8..])
            .all(u8::is_ascii_digit)
    {
        return false;
    }
    let year: u16 = value[..4].parse().unwrap();
    let month: u8 = value[5..7].parse().unwrap();
    let day: u8 = value[8..].parse().unwrap();
    let days = match month {
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return false,
    };
    year != 0 && (1..=days).contains(&day)
}

fn encode(value: impl Serialize) -> String {
    serde_json::to_value(value)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}

fn decode<T: DeserializeOwned>(row: &PgRow, column: &str) -> Result<T, WorkbenchError> {
    serde_json::from_value(serde_json::Value::String(row.try_get(column)?))
        .map_err(|_| WorkbenchError::Invalid)
}

fn record_from_row(row: PgRow) -> Result<Record, WorkbenchError> {
    Ok(Record {
        record_id: row.try_get("record_id")?,
        input: Input {
            subject_actor_id: row.try_get("subject_actor_id")?,
            audience_actor_id: row.try_get("audience_actor_id")?,
            cue: decode(&row, "cue")?,
            observation: decode(&row, "observation")?,
            audience_scope: row.try_get("audience_scope")?,
            effective_from: row.try_get("effective_from")?,
            effective_through: row.try_get("effective_through")?,
            available_on: row.try_get("available_on")?,
            claim_object_id: row.try_get("claim_object_id")?,
            representation_id: row.try_get("representation_id")?,
        },
        recorded_by: row.try_get("recorded_by")?,
    })
}

async fn ensure_scope(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    subject: Uuid,
    audience: Uuid,
) -> Result<(), WorkbenchError> {
    if subject == audience {
        return Err(WorkbenchError::Invalid);
    }
    let actors_visible: bool = sqlx::query_scalar(
        "SELECT count(*) = 2 FROM niccolo.actors
         WHERE workspace_id=$1 AND actor_id IN ($2, $3)",
    )
    .bind(workspace)
    .bind(subject)
    .bind(audience)
    .fetch_one(&mut **tx)
    .await?;
    if actors_visible && situation_visible(tx, workspace, situation).await? {
        return Ok(());
    }
    Err(WorkbenchError::Invisible)
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    author: Uuid,
    record_id: Uuid,
    input: Input,
) -> Result<bool, WorkbenchError> {
    if !valid_text(&input.audience_scope, 2048)
        || !calendar_date(&input.effective_from)
        || !calendar_date(&input.effective_through)
        || !calendar_date(&input.available_on)
        || input.effective_from > input.effective_through
    {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    ensure_scope(
        &mut tx,
        workspace,
        situation,
        input.subject_actor_id,
        input.audience_actor_id,
    )
    .await?;
    let written = sqlx::query(
        "INSERT INTO niccolo.public_prominence_records
           (workspace_id, situation_id, record_id, subject_actor_id, audience_actor_id, cue,
            observation, audience_scope, effective_from, effective_through, available_on,
            claim_object_id, representation_id, recorded_by)
         SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9::text::date,$10::text::date,$11::text::date,$12,$13,$14
         WHERE EXISTS (
           SELECT 1 FROM niccolo.workbench_artifact_provenance p
           JOIN niccolo.epistemic_situation_propositions m ON
             (m.workspace_id,m.situation_id,m.proposition_id) = (p.workspace_id,p.situation_id,p.object_id)
           WHERE (p.workspace_id,p.situation_id,p.object_id,p.representation_id) = ($1,$2,$12,$13)
         )
         ON CONFLICT (workspace_id, situation_id, record_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(record_id)
    .bind(input.subject_actor_id)
    .bind(input.audience_actor_id)
    .bind(encode(input.cue))
    .bind(encode(input.observation))
    .bind(&input.audience_scope)
    .bind(&input.effective_from)
    .bind(&input.effective_through)
    .bind(&input.available_on)
    .bind(input.claim_object_id)
    .bind(input.representation_id)
    .bind(author)
    .execute(&mut *tx)
    .await?
    .rows_affected() == 1;
    if !written {
        let sql = format!(
            "SELECT {RECORD_COLUMNS} FROM niccolo.public_prominence_records
             WHERE workspace_id=$1 AND situation_id=$2 AND record_id=$3"
        );
        let stored = sqlx::query(&sql)
            .bind(workspace)
            .bind(situation)
            .bind(record_id)
            .fetch_optional(&mut *tx)
            .await?
            .map(record_from_row)
            .transpose()?
            .ok_or(WorkbenchError::Invisible)?;
        if stored.input != input || stored.recorded_by != author {
            return Err(WorkbenchError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(written)
}

pub async fn lookup(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    query: Lookup,
) -> Result<LookupResult, WorkbenchError> {
    if !calendar_date(&query.on) || !calendar_date(&query.known_through) {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, true).await?;
    ensure_scope(
        &mut tx,
        workspace,
        situation,
        query.subject_actor_id,
        query.audience_actor_id,
    )
    .await?;
    let sql = format!(
        "SELECT {RECORD_COLUMNS} FROM niccolo.public_prominence_records
         WHERE workspace_id=$1 AND situation_id=$2 AND subject_actor_id=$3 AND audience_actor_id=$4
           AND $5::text::date BETWEEN effective_from AND effective_through
           AND available_on <= $6::text::date AND ($7::text IS NULL OR cue=$7)
         ORDER BY available_on, recorded_at, record_id LIMIT $8"
    );
    let mut records = sqlx::query(&sql)
        .bind(workspace)
        .bind(situation)
        .bind(query.subject_actor_id)
        .bind(query.audience_actor_id)
        .bind(&query.on)
        .bind(&query.known_through)
        .bind(query.cue.map(encode))
        .bind(LOOKUP_LIMIT + 1)
        .fetch_all(&mut *tx)
        .await?
        .into_iter()
        .map(record_from_row)
        .collect::<Result<Vec<_>, _>>()?;
    tx.commit().await?;
    let truncated = records.len() > LOOKUP_LIMIT as usize;
    records.truncate(LOOKUP_LIMIT as usize);
    let coverage = if records.is_empty() {
        Coverage::NoRecordedReports
    } else {
        Coverage::RecordedReports
    };
    Ok(LookupResult {
        query,
        coverage,
        records,
        truncated,
    })
}
