//! Source-backed assertions about the order of one exposure and one testimony.
use super::{
    WorkbenchError, detail_exposure::DetailSource, public_prominence::calendar_date, scoped,
};
use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Direction {
    ExposureBeforeTestimony,
    ExposureAfterTestimony,
}

impl Direction {
    fn db_value(self) -> &'static str {
        match self {
            Self::ExposureBeforeTestimony => "EXPOSURE_BEFORE_TESTIMONY",
            Self::ExposureAfterTestimony => "EXPOSURE_AFTER_TESTIMONY",
        }
    }
}

#[derive(Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub exposure_record_id: Uuid,
    pub testimony_representation_id: Uuid,
    pub candidate_representation_id: Uuid,
    pub direction: Direction,
    pub source: DetailSource,
    pub available_on: String,
}

#[derive(Serialize)]
pub struct Report {
    pub report_id: Uuid,
    #[serde(flatten)]
    pub input: Input,
    pub recorded_by: Uuid,
}

const REPORT_COLUMNS: &str = "
  report_id, exposure_record_id, testimony_representation_id, candidate_representation_id,
  direction, source_object_id, source_representation_id,
  to_char(available_on, 'YYYY-MM-DD') AS available_on, recorded_by";

fn read_report(row: PgRow) -> Result<Report, WorkbenchError> {
    let direction = match row.try_get::<&str, _>("direction")? {
        "EXPOSURE_BEFORE_TESTIMONY" => Direction::ExposureBeforeTestimony,
        "EXPOSURE_AFTER_TESTIMONY" => Direction::ExposureAfterTestimony,
        _ => return Err(WorkbenchError::Invalid),
    };
    Ok(Report {
        report_id: row.try_get("report_id")?,
        input: Input {
            exposure_record_id: row.try_get("exposure_record_id")?,
            testimony_representation_id: row.try_get("testimony_representation_id")?,
            candidate_representation_id: row.try_get("candidate_representation_id")?,
            direction,
            source: DetailSource {
                object_id: row.try_get("source_object_id")?,
                representation_id: row.try_get("source_representation_id")?,
            },
            available_on: row.try_get("available_on")?,
        },
        recorded_by: row.try_get("recorded_by")?,
    })
}

pub async fn record(
    pool: &PgPool,
    workspace: Uuid,
    situation: Uuid,
    author: Uuid,
    report_id: Uuid,
    input: Input,
) -> Result<bool, WorkbenchError> {
    if report_id.is_nil() || !calendar_date(&input.available_on) {
        return Err(WorkbenchError::Invalid);
    }
    let mut tx = scoped(pool, workspace, false).await?;
    let created = sqlx::query(
        "INSERT INTO niccolo.exposure_order_reports
          (workspace_id, situation_id, report_id, exposure_record_id, testimony_representation_id,
           candidate_representation_id, direction, source_object_id, source_representation_id,
           available_on, recorded_by)
         SELECT $1,$2,$3,$4,$5,$6,$7,$8,$9,$10::text::date,$11
         WHERE EXISTS (
           SELECT 1 FROM niccolo.public_prominence_records
           WHERE (workspace_id,situation_id,record_id)=($1,$2,$4)
             AND observation='EXPOSURE_REPORTED'
         ) AND EXISTS (
           SELECT 1 FROM niccolo.situation_artifact_representations
           WHERE (workspace_id,situation_id,representation_id)=($1,$2,$5)
         ) AND EXISTS (
           SELECT 1 FROM niccolo.situation_artifact_representations
           WHERE (workspace_id,situation_id,representation_id)=($1,$2,$6)
         ) AND EXISTS (
           SELECT 1 FROM niccolo.workbench_artifact_provenance
           WHERE (workspace_id,situation_id,object_id,representation_id)=($1,$2,$8,$9)
         )
         ON CONFLICT (workspace_id, situation_id, report_id) DO NOTHING",
    )
    .bind(workspace)
    .bind(situation)
    .bind(report_id)
    .bind(input.exposure_record_id)
    .bind(input.testimony_representation_id)
    .bind(input.candidate_representation_id)
    .bind(input.direction.db_value())
    .bind(input.source.object_id)
    .bind(input.source.representation_id)
    .bind(&input.available_on)
    .bind(author)
    .execute(&mut *tx)
    .await?
    .rows_affected()
        == 1;
    if !created {
        let sql = format!(
            "SELECT {REPORT_COLUMNS} FROM niccolo.exposure_order_reports
             WHERE (workspace_id,situation_id,report_id)=($1,$2,$3)"
        );
        let previous = sqlx::query(&sql)
            .bind(workspace)
            .bind(situation)
            .bind(report_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(WorkbenchError::Invisible)?;
        let previous = read_report(previous)?;
        if previous.input != input || previous.recorded_by != author {
            return Err(WorkbenchError::Conflict);
        }
    }
    tx.commit().await?;
    Ok(created)
}

pub(super) const REPORTS_PER_DIRECTION: usize = 50;

pub(super) async fn for_review(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    situation: Uuid,
    input: &super::detail_exposure::Input,
) -> Result<(Vec<Report>, bool), WorkbenchError> {
    // Keep a bounded sample from each direction so a minority report still
    // participates in the comparison when the other direction fills its page.
    let sql = format!(
        "SELECT {REPORT_COLUMNS}
         FROM (VALUES ('EXPOSURE_BEFORE_TESTIMONY'), ('EXPOSURE_AFTER_TESTIMONY')) AS d(value)
         CROSS JOIN LATERAL (
           SELECT * FROM niccolo.exposure_order_reports
           WHERE (workspace_id,situation_id,exposure_record_id,
                  testimony_representation_id,candidate_representation_id)=($1,$2,$3,$4,$5)
             AND direction=d.value AND available_on <= $6::text::date
           ORDER BY available_on,report_id LIMIT $7
         ) report
         ORDER BY direction,available_on,report_id"
    );
    let rows = sqlx::query(&sql)
        .bind(workspace)
        .bind(situation)
        .bind(input.exposure_record_id)
        .bind(input.testimony_detail.representation_id)
        .bind(input.candidate_source.representation_id)
        .bind(&input.known_through)
        .bind((REPORTS_PER_DIRECTION + 1) as i64)
        .fetch_all(&mut **tx)
        .await?;
    let mut before = 0;
    let mut after = 0;
    let mut reports = Vec::with_capacity(rows.len());
    for row in rows {
        let report = read_report(row)?;
        let count = match report.input.direction {
            Direction::ExposureBeforeTestimony => &mut before,
            Direction::ExposureAfterTestimony => &mut after,
        };
        *count += 1;
        if *count <= REPORTS_PER_DIRECTION {
            reports.push(report);
        }
    }
    Ok((
        reports,
        before > REPORTS_PER_DIRECTION || after > REPORTS_PER_DIRECTION,
    ))
}
