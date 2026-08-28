use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use sim_core::{
    FuelProfile, GameId, LapRecord, LapSummary, LeaderboardEntry, LeaderboardTrackOption,
    SectorTimes, SessionRecord, TrackLapOption,
};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const LEADERBOARD_SLOTS_PER_DRIVER: usize = 3;

pub struct Database {
    conn: Connection,
    data_dir: PathBuf,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let data_dir = path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).context("open sqlite database")?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        let db = Self { conn, data_dir };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS sessions (
                id TEXT PRIMARY KEY NOT NULL,
                game TEXT NOT NULL,
                track_id TEXT NOT NULL DEFAULT '',
                track TEXT NOT NULL,
                car TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                game_version TEXT NOT NULL,
                player_name TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS laps (
                id TEXT PRIMARY KEY NOT NULL,
                session_id TEXT NOT NULL,
                lap_number INTEGER NOT NULL,
                lap_time_ms INTEGER NOT NULL,
                valid INTEGER NOT NULL,
                is_best INTEGER NOT NULL,
                is_pinned INTEGER NOT NULL,
                sectors_json TEXT NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id)
            );

            CREATE TABLE IF NOT EXISTS lap_files (
                lap_id TEXT PRIMARY KEY NOT NULL,
                parquet_path TEXT NOT NULL,
                sample_rate_hz REAL NOT NULL,
                channel_manifest_json TEXT NOT NULL,
                FOREIGN KEY(lap_id) REFERENCES laps(id)
            );
            "#,
        )?;
        self.ensure_track_id_column()?;
        self.ensure_lap_extras_columns()?;
        self.ensure_lap_stint_column()?;
        self.ensure_lap_stint_break_column()?;
        self.ensure_leaderboard_table()?;
        self.sync_leaderboard_slots_if_needed()?;
        self.backfill_leaderboard_if_empty()?;
        self.ensure_session_fuel_stats_table()?;
        self.backfill_session_fuel_stats()?;
        self.prune_lapless_sessions()?;
        Ok(())
    }

    fn ensure_leaderboard_table(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS leaderboard_laps (
                id TEXT PRIMARY KEY NOT NULL,
                game TEXT NOT NULL,
                track_id TEXT NOT NULL DEFAULT '',
                track TEXT NOT NULL,
                car TEXT NOT NULL,
                player_name TEXT NOT NULL,
                player_key TEXT NOT NULL,
                track_key TEXT NOT NULL,
                lap_time_ms INTEGER NOT NULL,
                valid INTEGER NOT NULL,
                sectors_json TEXT NOT NULL,
                source_session_id TEXT NOT NULL,
                source_lap_id TEXT NOT NULL,
                parquet_path TEXT NOT NULL DEFAULT '',
                recorded_at TEXT NOT NULL,
                tyre_compound TEXT,
                tc_level INTEGER,
                abs_level INTEGER,
                fuel_used_l REAL,
                UNIQUE(source_lap_id)
            );
            CREATE INDEX IF NOT EXISTS idx_leaderboard_combo
                ON leaderboard_laps (game, track_key, player_key, lap_time_ms);
            CREATE TABLE IF NOT EXISTS app_meta (
                key TEXT PRIMARY KEY NOT NULL,
                value TEXT NOT NULL
            );
            "#,
        )?;
        self.migrate_leaderboard_drop_player_unique()?;
        self.ensure_leaderboard_lap_number_column()?;
        Ok(())
    }

    fn ensure_leaderboard_lap_number_column(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(leaderboard_laps)")?;
        let has_lap_number = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == "lap_number");
        if !has_lap_number {
            self.conn.execute(
                "ALTER TABLE leaderboard_laps ADD COLUMN lap_number INTEGER NOT NULL DEFAULT 0",
                [],
            )?;
        }
        Ok(())
    }

    fn migrate_leaderboard_drop_player_unique(&self) -> Result<()> {
        let sql: String = self.conn.query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'leaderboard_laps'",
            [],
            |row| row.get(0),
        )?;
        if !sql.contains("UNIQUE(game, track_key, player_key)")
            && !sql.contains("UNIQUE (game, track_key, player_key)")
        {
            return Ok(());
        }
        self.conn.execute_batch(
            r#"
            CREATE TABLE leaderboard_laps_new (
                id TEXT PRIMARY KEY NOT NULL,
                game TEXT NOT NULL,
                track_id TEXT NOT NULL DEFAULT '',
                track TEXT NOT NULL,
                car TEXT NOT NULL,
                player_name TEXT NOT NULL,
                player_key TEXT NOT NULL,
                track_key TEXT NOT NULL,
                lap_time_ms INTEGER NOT NULL,
                valid INTEGER NOT NULL,
                sectors_json TEXT NOT NULL,
                source_session_id TEXT NOT NULL,
                source_lap_id TEXT NOT NULL,
                parquet_path TEXT NOT NULL DEFAULT '',
                recorded_at TEXT NOT NULL,
                tyre_compound TEXT,
                tc_level INTEGER,
                abs_level INTEGER,
                fuel_used_l REAL,
                UNIQUE(source_lap_id)
            );
            INSERT INTO leaderboard_laps_new SELECT * FROM leaderboard_laps;
            DROP TABLE leaderboard_laps;
            ALTER TABLE leaderboard_laps_new RENAME TO leaderboard_laps;
            CREATE INDEX IF NOT EXISTS idx_leaderboard_combo
                ON leaderboard_laps (game, track_key, player_key, lap_time_ms);
            "#,
        )?;
        Ok(())
    }

    fn sync_leaderboard_slots_if_needed(&self) -> Result<()> {
        let current: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM app_meta WHERE key = 'leaderboard_slots'",
                [],
                |row| row.get(0),
            )
            .ok();
        let expected = LEADERBOARD_SLOTS_PER_DRIVER.to_string();
        if current.as_deref() == Some(expected.as_str()) {
            return Ok(());
        }
        self.sync_leaderboard_from_sessions()?;
        self.conn.execute(
            "INSERT INTO app_meta (key, value) VALUES ('leaderboard_slots', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![expected],
        )?;
        Ok(())
    }

    fn sync_leaderboard_from_sessions(&self) -> Result<()> {
        self.backfill_leaderboard(true)
    }

    fn ensure_lap_extras_columns(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(laps)")?;
        let columns: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        if !columns.iter().any(|c| c == "tyre_compound") {
            self.conn.execute("ALTER TABLE laps ADD COLUMN tyre_compound TEXT", [])?;
        }
        if !columns.iter().any(|c| c == "tc_level") {
            self.conn.execute("ALTER TABLE laps ADD COLUMN tc_level INTEGER", [])?;
        }
        if !columns.iter().any(|c| c == "abs_level") {
            self.conn.execute("ALTER TABLE laps ADD COLUMN abs_level INTEGER", [])?;
        }
        if !columns.iter().any(|c| c == "fuel_used_l") {
            self.conn.execute("ALTER TABLE laps ADD COLUMN fuel_used_l REAL", [])?;
        }
        Ok(())
    }

    fn ensure_lap_stint_column(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(laps)")?;
        let has_stint = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == "stint");
        if !has_stint {
            self.conn.execute(
                "ALTER TABLE laps ADD COLUMN stint INTEGER NOT NULL DEFAULT 1",
                [],
            )?;
        }
        Ok(())
    }

    fn ensure_lap_stint_break_column(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(laps)")?;
        let has_col = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == "stint_break_s");
        if !has_col {
            self.conn
                .execute("ALTER TABLE laps ADD COLUMN stint_break_s INTEGER", [])?;
        }
        Ok(())
    }

    /// Startup sweep for sessions that never recorded a lap. Runs before the
    /// recorder starts, so any lapless session is a stale artifact (app killed
    /// mid-menu, pre-gating track flips). Leaderboard/fuel-stats rows are only
    /// written from real laps, so nothing of value is lost.
    fn prune_lapless_sessions(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM sessions
             WHERE NOT EXISTS (SELECT 1 FROM laps WHERE laps.session_id = sessions.id)",
            [],
        )?;
        Ok(())
    }

    fn ensure_track_id_column(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("PRAGMA table_info(sessions)")?;
        let has_track_id = stmt
            .query_map([], |row| row.get::<_, String>(1))?
            .filter_map(Result::ok)
            .any(|name| name == "track_id");
        if !has_track_id {
            self.conn.execute(
                "ALTER TABLE sessions ADD COLUMN track_id TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        Ok(())
    }

    fn ensure_session_fuel_stats_table(&self) -> Result<()> {
        self.conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS session_fuel_stats (
                session_id TEXT PRIMARY KEY NOT NULL,
                game TEXT NOT NULL,
                car TEXT NOT NULL,
                track TEXT NOT NULL,
                lap_time_sum_ms INTEGER NOT NULL DEFAULT 0,
                valid_lap_count INTEGER NOT NULL DEFAULT 0,
                fuel_sum_l REAL NOT NULL DEFAULT 0,
                fuel_lap_count INTEGER NOT NULL DEFAULT 0,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_session_fuel_stats_combo
                ON session_fuel_stats (game, car, track);
            "#,
        )?;
        Ok(())
    }

    fn backfill_session_fuel_stats(&self) -> Result<()> {
        let mut stmt = self.conn.prepare("SELECT id FROM sessions")?;
        let ids: Vec<String> = stmt
            .query_map([], |row| row.get(0))?
            .filter_map(Result::ok)
            .collect();
        drop(stmt);
        for id in ids {
            if let Ok(uuid) = Uuid::parse_str(&id) {
                if let Err(err) = self.refresh_session_fuel_stats(uuid) {
                    tracing::warn!(
                        session_id = %id,
                        error = %err,
                        "failed to backfill session fuel stats"
                    );
                }
            }
        }
        Ok(())
    }

    fn refresh_session_fuel_stats(&self, session_id: Uuid) -> Result<()> {
        let mut session_stmt = self.conn.prepare(
            "SELECT game, car, track FROM sessions WHERE id = ?1",
        )?;
        let (game, car, track) = match session_stmt.query_row(params![session_id.to_string()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        }) {
            Ok(row) => row,
            Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(()),
            Err(err) => return Err(err.into()),
        };
        drop(session_stmt);

        let mut lap_stmt = self.conn.prepare(
            "SELECT lap_time_ms, valid, fuel_used_l FROM laps WHERE session_id = ?1",
        )?;
        let laps: Vec<(u32, i32, Option<f64>)> = lap_stmt
            .query_map(params![session_id.to_string()], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .filter_map(Result::ok)
            .collect();
        drop(lap_stmt);

        let valid: Vec<(u32, Option<f64>)> = laps
            .into_iter()
            .filter(|(_, valid, _)| *valid != 0)
            .map(|(t, _, fuel)| (t, fuel))
            .collect();
        if valid.is_empty() {
            return Ok(());
        }

        let valid_lap_count = valid.len() as i64;
        let lap_time_sum_ms: i64 = valid.iter().map(|(t, _)| i64::from(*t)).sum();
        let fuel_values: Vec<f64> = valid
            .iter()
            .filter_map(|(_, fuel)| {
                fuel.filter(|v| v.is_finite() && *v > 0.0)
            })
            .collect();
        let (fuel_sum_l, fuel_lap_count) = match average_excluding_extreme_outliers(&fuel_values) {
            Some((avg, count)) => (avg * count as f64, count as i64),
            None => (0.0, 0),
        };

        self.conn.execute(
            r#"
            INSERT INTO session_fuel_stats (
                session_id, game, car, track, lap_time_sum_ms, valid_lap_count,
                fuel_sum_l, fuel_lap_count, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(session_id) DO UPDATE SET
                game = excluded.game,
                car = excluded.car,
                track = excluded.track,
                lap_time_sum_ms = excluded.lap_time_sum_ms,
                valid_lap_count = excluded.valid_lap_count,
                fuel_sum_l = excluded.fuel_sum_l,
                fuel_lap_count = excluded.fuel_lap_count,
                updated_at = excluded.updated_at
            "#,
            params![
                session_id.to_string(),
                game,
                car,
                track,
                lap_time_sum_ms,
                valid_lap_count,
                fuel_sum_l,
                fuel_lap_count,
                Utc::now().to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_fuel_profiles(&self) -> Result<Vec<FuelProfile>> {
        let mut stmt = self.conn.prepare(
            r#"
            SELECT game, car, track,
                   CAST(ROUND(SUM(lap_time_sum_ms) * 1.0 / SUM(valid_lap_count)) AS INTEGER) AS avg_lap_ms,
                   CASE WHEN SUM(fuel_lap_count) > 0
                        THEN SUM(fuel_sum_l) / SUM(fuel_lap_count)
                        ELSE NULL END AS avg_fuel_l
            FROM session_fuel_stats
            WHERE valid_lap_count > 0
            GROUP BY game, car, track
            ORDER BY track COLLATE NOCASE, car COLLATE NOCASE
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            let game: String = row.get(0)?;
            Ok(FuelProfile {
                game: serde_json::from_str(&game).unwrap_or(GameId::Acc),
                car: row.get(1)?,
                track: row.get(2)?,
                avg_lap_time_ms: row.get(3)?,
                avg_fuel_used_l: row.get::<_, Option<f64>>(4)?.map(|v| v as f32),
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn create_session(
        &self,
        game: GameId,
        track_id: &str,
        track: &str,
        car: &str,
        game_version: &str,
        player_name: &str,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        let started_at = Utc::now();
        self.conn.execute(
            "INSERT INTO sessions (id, game, track_id, track, car, started_at, ended_at, game_version, player_name) VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7, ?8)",
            params![
                id.to_string(),
                serde_json::to_string(&game)?,
                track_id,
                track,
                car,
                started_at.to_rfc3339(),
                game_version,
                player_name,
            ],
        )?;
        Ok(id)
    }

    pub fn finalize_session(&self, session_id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET ended_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), session_id.to_string()],
        )?;
        // A session that never recorded a lap (menu navigation, a quick track
        // switch, an abandoned load) is noise — drop it rather than leave a
        // "0 laps" card. No FK references sessions(id) except laps.session_id,
        // and there are none here.
        self.conn.execute(
            "DELETE FROM sessions
             WHERE id = ?1 AND NOT EXISTS (SELECT 1 FROM laps WHERE session_id = ?1)",
            params![session_id.to_string()],
        )?;
        Ok(())
    }

    pub fn update_session_metadata(
        &self,
        session_id: Uuid,
        track_id: &str,
        track: &str,
        car: &str,
        game_version: &str,
        player_name: &str,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET track_id = ?1, track = ?2, car = ?3, game_version = ?4, player_name = ?5 WHERE id = ?6",
            params![
                track_id,
                track,
                car,
                game_version,
                player_name,
                session_id.to_string(),
            ],
        )?;
        let _ = self.refresh_session_fuel_stats(session_id);
        Ok(())
    }

    pub fn insert_lap(
        &self,
        session_id: Uuid,
        summary: &LapSummary,
        parquet_path: &str,
        sample_rate_hz: f32,
        channel_manifest_json: &str,
    ) -> Result<Uuid> {
        let id = Uuid::new_v4();
        self.insert_lap_with_id(
            id,
            session_id,
            summary,
            parquet_path,
            sample_rate_hz,
            channel_manifest_json,
            1,
            None,
        )?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_lap_with_id(
        &self,
        id: Uuid,
        session_id: Uuid,
        summary: &LapSummary,
        parquet_path: &str,
        sample_rate_hz: f32,
        channel_manifest_json: &str,
        stint: u32,
        stint_break_s: Option<u32>,
    ) -> Result<()> {
        let sectors_json = serde_json::to_string(&summary.sectors)?;
        let stint = stint.max(1);
        self.conn.execute(
            "INSERT INTO laps (id, session_id, lap_number, lap_time_ms, valid, is_best, is_pinned, sectors_json, tyre_compound, tc_level, abs_level, fuel_used_l, stint, stint_break_s) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                id.to_string(),
                session_id.to_string(),
                summary.lap_number,
                summary.lap_time_ms,
                summary.valid as i32,
                sectors_json,
                summary.tyre_compound,
                summary.tc_level,
                summary.abs_level,
                summary.fuel_used_l,
                stint,
                stint_break_s,
            ],
        )?;
        self.conn.execute(
            "INSERT INTO lap_files (lap_id, parquet_path, sample_rate_hz, channel_manifest_json) VALUES (?1, ?2, ?3, ?4)",
            params![
                id.to_string(),
                parquet_path,
                sample_rate_hz,
                channel_manifest_json,
            ],
        )?;
        self.refresh_best_lap(session_id)?;
        if let Err(err) = self.consider_leaderboard_lap(session_id, id, summary, parquet_path) {
            tracing::warn!(
                lap_id = %id,
                session_id = %session_id,
                error = %err,
                "failed to update persistent leaderboard"
            );
        }
        if let Err(err) = self.refresh_session_fuel_stats(session_id) {
            tracing::warn!(
                session_id = %session_id,
                error = %err,
                "failed to update persistent fuel stats"
            );
        }
        Ok(())
    }

    pub fn refresh_best_lap(&self, session_id: Uuid) -> Result<()> {
        self.conn.execute(
            "UPDATE laps SET is_best = 0 WHERE session_id = ?1",
            params![session_id.to_string()],
        )?;
        self.conn.execute(
            "UPDATE laps SET is_best = 1 WHERE id = (
                SELECT id FROM laps WHERE session_id = ?1 AND valid = 1
                ORDER BY lap_time_ms ASC LIMIT 1
            )",
            params![session_id.to_string()],
        )?;
        Ok(())
    }

    pub fn set_lap_pinned(&self, lap_id: Uuid, pinned: bool) -> Result<()> {
        self.conn.execute(
            "UPDATE laps SET is_pinned = ?1 WHERE id = ?2",
            params![pinned as i32, lap_id.to_string()],
        )?;
        Ok(())
    }

    pub fn list_sessions(&self) -> Result<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.game, s.track_id, s.track, s.car, s.started_at, s.ended_at, s.game_version, s.player_name,
                    (SELECT COUNT(*) FROM laps l WHERE l.session_id = s.id) as lap_count,
                    (SELECT MIN(lap_time_ms) FROM laps l WHERE l.session_id = s.id AND l.valid = 1) as best_lap
             FROM sessions s
             WHERE s.ended_at IS NULL
                OR EXISTS (SELECT 1 FROM laps l WHERE l.session_id = s.id)
             ORDER BY s.started_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let game: String = row.get(1)?;
            let started_at: String = row.get(5)?;
            let ended_at: Option<String> = row.get(6)?;
            Ok(SessionRecord {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                game: serde_json::from_str(&game).unwrap_or(GameId::Acc),
                track_id: row.get(2)?,
                track: row.get(3)?,
                car: row.get(4)?,
                started_at: DateTime::parse_from_rfc3339(&started_at)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
                ended_at: ended_at.and_then(|v| {
                    DateTime::parse_from_rfc3339(&v)
                        .ok()
                        .map(|d| d.with_timezone(&Utc))
                }),
                game_version: row.get(7)?,
                player_name: row.get(8)?,
                lap_count: row.get(9)?,
                best_lap_time_ms: row.get(10)?,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn list_laps(&self, session_id: Uuid) -> Result<Vec<LapRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT l.id, l.session_id, l.lap_number, l.lap_time_ms, l.valid, l.is_best, l.is_pinned, l.sectors_json, f.sample_rate_hz, l.tyre_compound, l.tc_level, l.abs_level, l.fuel_used_l, l.stint, l.stint_break_s
             FROM laps l
             JOIN lap_files f ON f.lap_id = l.id
             WHERE l.session_id = ?1
             ORDER BY l.stint ASC, l.lap_number ASC",
        )?;
        let rows = stmt.query_map(params![session_id.to_string()], |row| {
            let sectors_json: String = row.get(7)?;
            Ok(LapRecord {
                id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                session_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                lap_number: row.get(2)?,
                lap_time_ms: row.get(3)?,
                valid: row.get::<_, i32>(4)? != 0,
                is_best: row.get::<_, i32>(5)? != 0,
                is_pinned: row.get::<_, i32>(6)? != 0,
                sectors: serde_json::from_str(&sectors_json).unwrap_or(SectorTimes {
                    s1_ms: None,
                    s2_ms: None,
                    s3_ms: None,
                }),
                sample_rate_hz: row.get(8)?,
                tyre_compound: row.get(9)?,
                tc_level: row.get(10)?,
                abs_level: row.get(11)?,
                fuel_used_l: row.get(12)?,
                stint: {
                    let stint: i64 = row.get(13)?;
                    stint.max(1) as u32
                },
                stint_break_s: row
                    .get::<_, Option<i64>>(14)?
                    .map(|v| v.max(0) as u32),
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn get_lap_parquet_path(&self, lap_id: Uuid) -> Result<Option<String>> {
        let mut stmt = self.conn.prepare("SELECT parquet_path FROM lap_files WHERE lap_id = ?1")?;
        let mut rows = stmt.query(params![lap_id.to_string()])?;
        if let Some(row) = rows.next()? {
            return Ok(Some(row.get(0)?));
        }
        let mut stmt = self.conn.prepare(
            "SELECT parquet_path FROM leaderboard_laps
             WHERE source_lap_id = ?1 OR id = ?1
             LIMIT 1",
        )?;
        let mut rows = stmt.query(params![lap_id.to_string()])?;
        if let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            if !path.is_empty() {
                return Ok(Some(path));
            }
        }
        Ok(None)
    }

    pub fn update_lap_parquet_path(&self, lap_id: Uuid, path: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE lap_files SET parquet_path = ?1 WHERE lap_id = ?2",
            params![path, lap_id.to_string()],
        )?;
        Ok(())
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn delete_session(&self, session_id: Uuid, data_dir: &Path) -> Result<()> {
        self.salvage_leaderboard_parquet_for_session(session_id)?;
        self.conn.execute(
            "DELETE FROM lap_files WHERE lap_id IN (SELECT id FROM laps WHERE session_id = ?1)",
            params![session_id.to_string()],
        )?;
        self.conn.execute(
            "DELETE FROM laps WHERE session_id = ?1",
            params![session_id.to_string()],
        )?;
        self.conn.execute(
            "DELETE FROM sessions WHERE id = ?1",
            params![session_id.to_string()],
        )?;
        let session_dir = data_dir.join("sessions").join(session_id.to_string());
        if session_dir.exists() {
            std::fs::remove_dir_all(&session_dir).with_context(|| {
                format!("remove session directory {}", session_dir.display())
            })?;
        }
        Ok(())
    }

    fn salvage_leaderboard_parquet_for_session(&self, session_id: Uuid) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "SELECT id, parquet_path FROM leaderboard_laps WHERE source_session_id = ?1",
        )?;
        let rows: Vec<(String, String)> = stmt
            .query_map(params![session_id.to_string()], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })?
            .filter_map(Result::ok)
            .collect();
        drop(stmt);
        for (id, path) in rows {
            let dest_rel = format!("leaderboard/laps/{id}.parquet");
            let dest_abs = match crate::paths::resolve_data_relative(&self.data_dir, &dest_rel) {
                Ok(p) => p,
                Err(_) => continue,
            };
            if dest_abs.exists() && is_leaderboard_parquet(&path) {
                continue;
            }
            let parquet_path = self.persist_leaderboard_parquet(&path, &dest_rel);
            if parquet_path != path {
                self.conn.execute(
                    "UPDATE leaderboard_laps SET parquet_path = ?1 WHERE id = ?2",
                    params![parquet_path, id],
                )?;
            }
        }
        Ok(())
    }

    pub fn get_session(&self, session_id: Uuid) -> Result<Option<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.id, s.game, s.track_id, s.track, s.car, s.started_at, s.ended_at, s.game_version, s.player_name,
                    (SELECT COUNT(*) FROM laps l WHERE l.session_id = s.id) as lap_count,
                    (SELECT MIN(lap_time_ms) FROM laps l WHERE l.session_id = s.id AND l.valid = 1) as best_lap
             FROM sessions s WHERE s.id = ?1",
        )?;
        let mut rows = stmt.query(params![session_id.to_string()])?;
        let Some(row) = rows.next()? else {
            return Ok(None);
        };
        let game: String = row.get(1)?;
        let started_at: String = row.get(5)?;
        let ended_at: Option<String> = row.get(6)?;
        Ok(Some(SessionRecord {
            id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
            game: serde_json::from_str(&game).unwrap_or(GameId::Acc),
            track_id: row.get(2)?,
            track: row.get(3)?,
            car: row.get(4)?,
            started_at: DateTime::parse_from_rfc3339(&started_at)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now()),
            ended_at: ended_at.and_then(|v| {
                DateTime::parse_from_rfc3339(&v)
                    .ok()
                    .map(|d| d.with_timezone(&Utc))
            }),
            game_version: row.get(7)?,
            player_name: row.get(8)?,
            lap_count: row.get(9)?,
            best_lap_time_ms: row.get(10)?,
        }))
    }

    pub fn list_leaderboard_games(&self) -> Result<Vec<GameId>> {
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT game FROM leaderboard_laps ORDER BY game",
        )?;
        let rows = stmt.query_map([], |row| {
            let game: String = row.get(0)?;
            Ok(serde_json::from_str(&game).unwrap_or(GameId::Acc))
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn list_leaderboard_tracks(&self, game: GameId) -> Result<Vec<LeaderboardTrackOption>> {
        let game_json = serde_json::to_string(&game)?;
        let mut stmt = self.conn.prepare(
            "SELECT DISTINCT track_id, track FROM leaderboard_laps
             WHERE game = ?1
             ORDER BY track",
        )?;
        let rows = stmt.query_map(params![game_json], |row| {
            Ok(LeaderboardTrackOption {
                track_id: row.get(0)?,
                track: row.get(1)?,
            })
        })?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    pub fn get_leaderboard(
        &self,
        game: GameId,
        track_id: &str,
        track_name: &str,
    ) -> Result<Vec<LeaderboardEntry>> {
        let game_json = serde_json::to_string(&game)?;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT lb.player_name, lb.lap_time_ms, lb.valid, lb.source_session_id, lb.source_lap_id,
                   CASE WHEN s.id IS NOT NULL THEN 1 ELSE 0 END
            FROM leaderboard_laps lb
            LEFT JOIN sessions s ON s.id = lb.source_session_id
            WHERE lb.valid = 1
              AND lb.game = ?1
              AND (
                (?2 != '' AND lb.track_id = ?2)
                OR (?2 = '' AND lb.track = ?3)
              )
            ORDER BY lb.lap_time_ms ASC, lb.source_lap_id ASC
            "#,
        )?;
        let rows = stmt.query_map(params![game_json, track_id, track_name], |row| {
            Ok(LeaderboardEntry {
                rank: 0,
                place: 1,
                player_name: row.get(0)?,
                lap_time_ms: row.get(1)?,
                valid: row.get::<_, i32>(2)? != 0,
                session_id: Uuid::parse_str(&row.get::<_, String>(3)?).unwrap_or_default(),
                lap_id: Uuid::parse_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                session_exists: row.get::<_, i32>(5)? != 0,
            })
        })?;
        let entries: Vec<LeaderboardEntry> = rows.filter_map(Result::ok).collect();
        Ok(rank_leaderboard_entries(entries))
    }

    /// Promote a valid lap onto the persistent leaderboard if it is among this
    /// player's top 3 for the game/track. Copies parquet under `leaderboard/laps/`
    /// so the time (and samples) survive session deletion.
    pub fn consider_leaderboard_lap(
        &self,
        session_id: Uuid,
        lap_id: Uuid,
        summary: &LapSummary,
        source_parquet_rel: &str,
    ) -> Result<()> {
        if !summary.valid || summary.lap_time_ms == 0 {
            return Ok(());
        }
        let Some(session) = self.get_session(session_id)? else {
            anyhow::bail!("session not found for leaderboard upsert");
        };
        let player_key = normalize_player_key(&session.player_name);
        let track_key = make_track_key(&session.track_id, &session.track);
        let game_json = serde_json::to_string(&session.game)?;
        let sectors_json = serde_json::to_string(&summary.sectors)?;
        let recorded_at = session.started_at.to_rfc3339();

        let already: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM leaderboard_laps WHERE source_lap_id = ?1",
            params![lap_id.to_string()],
            |row| row.get(0),
        )?;
        if already > 0 {
            return Ok(());
        }

        let mut existing_stmt = self.conn.prepare(
            "SELECT id, lap_time_ms, parquet_path FROM leaderboard_laps
             WHERE game = ?1 AND track_key = ?2 AND player_key = ?3
             ORDER BY lap_time_ms ASC, source_lap_id ASC",
        )?;
        let existing: Vec<(String, u32, String)> = existing_stmt
            .query_map(
                params![&game_json, &track_key, &player_key],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, i64>(1)? as u32,
                        row.get::<_, String>(2)?,
                    ))
                },
            )?
            .filter_map(Result::ok)
            .collect();
        drop(existing_stmt);

        if existing.len() >= LEADERBOARD_SLOTS_PER_DRIVER {
            let Some((worst_id, worst_time, old_parquet)) = existing.last() else {
                return Ok(());
            };
            if summary.lap_time_ms >= *worst_time {
                return Ok(());
            }
            let dest_rel = format!("leaderboard/laps/{worst_id}.parquet");
            let parquet_path = self.persist_leaderboard_parquet(source_parquet_rel, &dest_rel);
            if old_parquet != &parquet_path && is_leaderboard_parquet(old_parquet) {
                self.remove_data_file(old_parquet);
            }
            self.conn.execute(
                r#"
                UPDATE leaderboard_laps SET
                    track_id = ?1,
                    track = ?2,
                    car = ?3,
                    player_name = ?4,
                    lap_time_ms = ?5,
                    valid = 1,
                    sectors_json = ?6,
                    source_session_id = ?7,
                    source_lap_id = ?8,
                    parquet_path = ?9,
                    recorded_at = ?10,
                    tyre_compound = ?11,
                    tc_level = ?12,
                    abs_level = ?13,
                    fuel_used_l = ?14,
                    lap_number = ?16
                WHERE id = ?15
                "#,
                params![
                    session.track_id,
                    session.track,
                    session.car,
                    session.player_name,
                    summary.lap_time_ms,
                    sectors_json,
                    session_id.to_string(),
                    lap_id.to_string(),
                    parquet_path,
                    recorded_at,
                    summary.tyre_compound,
                    summary.tc_level,
                    summary.abs_level,
                    summary.fuel_used_l,
                    worst_id,
                    summary.lap_number,
                ],
            )?;
            return Ok(());
        }

        let id = Uuid::new_v4();
        let dest_rel = format!("leaderboard/laps/{id}.parquet");
        let parquet_path = self.persist_leaderboard_parquet(source_parquet_rel, &dest_rel);
        self.conn.execute(
            r#"
            INSERT INTO leaderboard_laps (
                id, game, track_id, track, car, player_name, player_key, track_key,
                lap_time_ms, valid, sectors_json, source_session_id, source_lap_id,
                parquet_path, recorded_at, tyre_compound, tc_level, abs_level, fuel_used_l,
                lap_number
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            "#,
            params![
                id.to_string(),
                game_json,
                session.track_id,
                session.track,
                session.car,
                session.player_name,
                player_key,
                track_key,
                summary.lap_time_ms,
                sectors_json,
                session_id.to_string(),
                lap_id.to_string(),
                parquet_path,
                recorded_at,
                summary.tyre_compound,
                summary.tc_level,
                summary.abs_level,
                summary.fuel_used_l,
                summary.lap_number,
            ],
        )?;
        Ok(())
    }

    fn backfill_leaderboard_if_empty(&self) -> Result<()> {
        self.backfill_leaderboard(false)
    }

    fn backfill_leaderboard(&self, force: bool) -> Result<()> {
        if !force {
            let count: i64 =
                self.conn
                    .query_row("SELECT COUNT(*) FROM leaderboard_laps", [], |row| row.get(0))?;
            if count > 0 {
                return Ok(());
            }
        }
        let mut stmt = self.conn.prepare(
            r#"
            SELECT l.id, l.session_id, l.lap_number, l.lap_time_ms, l.valid, l.sectors_json,
                   l.tyre_compound, l.tc_level, l.abs_level, l.fuel_used_l, f.parquet_path
            FROM laps l
            JOIN lap_files f ON f.lap_id = l.id
            WHERE l.valid = 1
            ORDER BY l.lap_time_ms ASC, l.id ASC
            "#,
        )?;
        let rows = stmt.query_map([], |row| {
            let sectors_json: String = row.get(5)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                LapSummary {
                    lap_number: row.get::<_, i64>(2)? as u32,
                    lap_time_ms: row.get::<_, i64>(3)? as u32,
                    valid: row.get::<_, i32>(4)? != 0,
                    sectors: serde_json::from_str(&sectors_json).unwrap_or(SectorTimes {
                        s1_ms: None,
                        s2_ms: None,
                        s3_ms: None,
                    }),
                    tyre_compound: row.get(6)?,
                    tc_level: row.get(7)?,
                    abs_level: row.get(8)?,
                    fuel_used_l: row.get(9)?,
                },
                row.get::<_, String>(10)?,
            ))
        })?;
        let pending: Vec<(String, String, LapSummary, String)> =
            rows.filter_map(Result::ok).collect();
        drop(stmt);
        for (lap_id, session_id, summary, parquet_path) in pending {
            let Ok(lap_uuid) = Uuid::parse_str(&lap_id) else {
                continue;
            };
            let Ok(session_uuid) = Uuid::parse_str(&session_id) else {
                continue;
            };
            if let Err(err) =
                self.consider_leaderboard_lap(session_uuid, lap_uuid, &summary, &parquet_path)
            {
                tracing::warn!(
                    lap_id,
                    session_id,
                    error = %err,
                    "failed to backfill leaderboard lap"
                );
            }
        }
        Ok(())
    }

    fn persist_leaderboard_parquet(&self, source_rel: &str, dest_rel: &str) -> String {
        match self.copy_data_file(source_rel, dest_rel) {
            Ok(()) => dest_rel.to_string(),
            Err(err) => {
                tracing::warn!(
                    source = source_rel,
                    dest = dest_rel,
                    error = %err,
                    "failed to copy leaderboard parquet; keeping source path"
                );
                source_rel.to_string()
            }
        }
    }

    fn copy_data_file(&self, source_rel: &str, dest_rel: &str) -> Result<()> {
        let src = crate::paths::resolve_data_relative(&self.data_dir, source_rel)?;
        let dest = crate::paths::resolve_data_relative(&self.data_dir, dest_rel)?;
        if !src.exists() {
            anyhow::bail!("source parquet missing: {}", src.display());
        }
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::copy(&src, &dest)?;
        Ok(())
    }

    fn remove_data_file(&self, relative: &str) {
        if let Ok(path) = crate::paths::resolve_data_relative(&self.data_dir, relative) {
            let _ = std::fs::remove_file(path);
        }
    }

    pub fn list_track_laps(
        &self,
        game: GameId,
        track_id: &str,
        track_name: &str,
    ) -> Result<Vec<TrackLapOption>> {
        let game_json = serde_json::to_string(&game)?;
        let mut stmt = self.conn.prepare(
            r#"
            SELECT l.id, l.session_id, l.lap_number, l.lap_time_ms, l.valid,
                   s.player_name, s.car, s.started_at, l.sectors_json
            FROM laps l
            INNER JOIN sessions s ON s.id = l.session_id
            WHERE l.valid = 1
              AND s.game = ?1
              AND (
                (?2 != '' AND s.track_id = ?2)
                OR (?2 = '' AND s.track = ?3)
              )
            ORDER BY l.lap_time_ms ASC
            "#,
        )?;
        let session_rows = stmt.query_map(
            params![game_json, track_id, track_name],
            map_track_lap_row,
        )?;
        let mut laps: Vec<TrackLapOption> = session_rows.filter_map(Result::ok).collect();
        drop(stmt);

        let mut seen: std::collections::HashSet<Uuid> =
            laps.iter().map(|lap| lap.lap_id).collect();

        let mut board_stmt = self.conn.prepare(
            r#"
            SELECT lb.source_lap_id, lb.source_session_id, lb.lap_number, lb.lap_time_ms, lb.valid,
                   lb.player_name, lb.car, lb.recorded_at, lb.sectors_json
            FROM leaderboard_laps lb
            WHERE lb.valid = 1
              AND lb.game = ?1
              AND (
                (?2 != '' AND lb.track_id = ?2)
                OR (?2 = '' AND lb.track = ?3)
              )
            "#,
        )?;
        let board_rows = board_stmt.query_map(
            params![game_json, track_id, track_name],
            map_track_lap_row,
        )?;
        for lap in board_rows.filter_map(Result::ok) {
            if seen.insert(lap.lap_id) {
                laps.push(lap);
            }
        }
        laps.sort_by_key(|lap| lap.lap_time_ms);
        Ok(laps)
    }
}

fn normalize_player_key(name: &str) -> String {
    name.trim().to_lowercase()
}

fn make_track_key(track_id: &str, track: &str) -> String {
    if !track_id.is_empty() {
        format!("id:{track_id}")
    } else {
        format!("name:{}", track.trim().to_lowercase())
    }
}

fn is_leaderboard_parquet(relative: &str) -> bool {
    relative.starts_with("leaderboard/")
}

fn map_track_lap_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TrackLapOption> {
    let started_at: String = row.get(7)?;
    let sectors_json: String = row.get(8)?;
    Ok(TrackLapOption {
        lap_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
        session_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
        lap_number: row.get::<_, i64>(2)? as u32,
        lap_time_ms: row.get::<_, i64>(3)? as u32,
        valid: row.get::<_, i32>(4)? != 0,
        player_name: row.get(5)?,
        car: row.get(6)?,
        started_at: DateTime::parse_from_rfc3339(&started_at)
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now()),
        sectors: serde_json::from_str(&sectors_json).unwrap_or(SectorTimes {
            s1_ms: None,
            s2_ms: None,
            s3_ms: None,
        }),
    })
}

fn rank_leaderboard_entries(entries: Vec<LeaderboardEntry>) -> Vec<LeaderboardEntry> {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    entries
        .into_iter()
        .enumerate()
        .map(|(i, mut entry)| {
            let key = normalize_player_key(&entry.player_name);
            let place = counts.entry(key).or_insert(0);
            *place += 1;
            entry.rank = (i + 1) as u32;
            entry.place = *place;
            entry
        })
        .collect()
}

/// Tukey extreme-outlier fence multiplier (beyond 3×IQR). Matches the session review UI.
const EXTREME_OUTLIER_IQR_FACTOR: f64 = 3.0;

fn quantile_sorted(sorted: &[f64], q: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let pos = (sorted.len() - 1) as f64 * q;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let t = pos - lo as f64;
    sorted[lo] * (1.0 - t) + sorted[hi] * t
}

fn average_excluding_extreme_outliers(values: &[f64]) -> Option<(f64, usize)> {
    if values.is_empty() {
        return None;
    }
    let mut used = values.to_vec();
    if values.len() >= 4 {
        let mut sorted = used.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let q1 = quantile_sorted(&sorted, 0.25);
        let q3 = quantile_sorted(&sorted, 0.75);
        let iqr = q3 - q1;
        if iqr > 0.0 {
            let lower = q1 - EXTREME_OUTLIER_IQR_FACTOR * iqr;
            let upper = q3 + EXTREME_OUTLIER_IQR_FACTOR * iqr;
            let filtered: Vec<f64> = used
                .iter()
                .copied()
                .filter(|v| *v >= lower && *v <= upper)
                .collect();
            if !filtered.is_empty() {
                used = filtered;
            }
        }
    }
    let average = used.iter().sum::<f64>() / used.len() as f64;
    Some((average, used.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn open_temp() -> (tempfile::TempDir, Database) {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::open(&dir.path().join("simtelemetry.db")).unwrap();
        (dir, db)
    }

    fn dummy_parquet(data_dir: &Path, session_id: Uuid, lap_id: Uuid, marker: &[u8]) -> String {
        let rel = format!("sessions/{session_id}/laps/{lap_id}.parquet");
        let abs = data_dir.join(&rel);
        fs::create_dir_all(abs.parent().unwrap()).unwrap();
        fs::write(&abs, marker).unwrap();
        rel
    }

    fn summary(lap_time_ms: u32, valid: bool) -> LapSummary {
        LapSummary {
            lap_number: 1,
            lap_time_ms,
            valid,
            sectors: SectorTimes {
                s1_ms: Some(30_000),
                s2_ms: Some(40_000),
                s3_ms: Some(lap_time_ms.saturating_sub(70_000)),
            },
            tyre_compound: None,
            tc_level: None,
            abs_level: None,
            fuel_used_l: None,
        }
    }

    fn insert_valid_lap(
        db: &Database,
        data_dir: &Path,
        session_id: Uuid,
        lap_time_ms: u32,
        marker: &[u8],
    ) -> Uuid {
        let lap_id = Uuid::new_v4();
        let rel = dummy_parquet(data_dir, session_id, lap_id, marker);
        db.insert_lap_with_id(lap_id, session_id, &summary(lap_time_ms, true), &rel, 100.0, "{}", 1, None)
            .unwrap();
        lap_id
    }

    fn insert_fuel_lap(
        db: &Database,
        data_dir: &Path,
        session_id: Uuid,
        lap_time_ms: u32,
        fuel_used_l: f32,
        marker: &[u8],
    ) -> Uuid {
        let lap_id = Uuid::new_v4();
        let rel = dummy_parquet(data_dir, session_id, lap_id, marker);
        let mut summary = summary(lap_time_ms, true);
        summary.fuel_used_l = Some(fuel_used_l);
        db.insert_lap_with_id(lap_id, session_id, &summary, &rel, 100.0, "{}", 1, None)
            .unwrap();
        lap_id
    }

    #[test]
    fn valid_lap_appears_on_leaderboard() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        insert_valid_lap(&db, dir.path(), session, 110_000, b"pb");

        let games = db.list_leaderboard_games().unwrap();
        assert_eq!(games, vec![GameId::Acc]);
        let tracks = db.list_leaderboard_tracks(GameId::Acc).unwrap();
        assert_eq!(tracks[0].track_id, "monza");
        let board = db.get_leaderboard(GameId::Acc, "monza", "Monza").unwrap();
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].player_name, "Dmytro");
        assert_eq!(board[0].lap_time_ms, 110_000);
        assert!(board[0].session_exists);
        assert_eq!(board[0].rank, 1);
    }

    #[test]
    fn keeps_top_three_laps_and_drops_slower() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        insert_valid_lap(&db, dir.path(), session, 110_000, b"slow");
        insert_valid_lap(&db, dir.path(), session, 108_000, b"fast");
        insert_valid_lap(&db, dir.path(), session, 109_000, b"mid");
        insert_valid_lap(&db, dir.path(), session, 111_000, b"dropped");

        let board = db.get_leaderboard(GameId::Acc, "monza", "Monza").unwrap();
        let times: Vec<u32> = board.iter().map(|e| e.lap_time_ms).collect();
        assert_eq!(times, vec![108_000, 109_000, 110_000]);
        assert_eq!(
            board.iter().map(|e| e.rank).collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
        assert!(board.iter().all(|e| e.player_name == "Dmytro"));

        insert_valid_lap(&db, dir.path(), session, 107_000, b"new-pb");
        let board = db.get_leaderboard(GameId::Acc, "monza", "Monza").unwrap();
        let times: Vec<u32> = board.iter().map(|e| e.lap_time_ms).collect();
        assert_eq!(times, vec![107_000, 108_000, 109_000]);
    }

    #[test]
    fn invalid_laps_are_ignored() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        let lap_id = Uuid::new_v4();
        let rel = dummy_parquet(dir.path(), session, lap_id, b"inv");
        db.insert_lap_with_id(lap_id, session, &summary(90_000, false), &rel, 100.0, "{}", 1, None)
            .unwrap();

        assert!(db.list_leaderboard_games().unwrap().is_empty());
        assert!(db.get_leaderboard(GameId::Acc, "monza", "Monza").unwrap().is_empty());
    }

    #[test]
    fn deleting_session_keeps_leaderboard_time_and_parquet() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        let lap_id = insert_valid_lap(&db, dir.path(), session, 110_000, b"keep-me");

        db.delete_session(session, dir.path()).unwrap();
        assert!(db.get_session(session).unwrap().is_none());
        assert!(db.list_laps(session).unwrap().is_empty());

        let board = db.get_leaderboard(GameId::Acc, "monza", "Monza").unwrap();
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].lap_time_ms, 110_000);
        assert_eq!(board[0].player_name, "Dmytro");
        assert!(!board[0].session_exists);
        assert_eq!(board[0].lap_id, lap_id);

        let stored = db.get_lap_parquet_path(lap_id).unwrap().expect("parquet path");
        assert!(stored.starts_with("leaderboard/"));
        let abs = dir.path().join(&stored);
        assert!(abs.exists());
        assert_eq!(fs::read(&abs).unwrap(), b"keep-me");

        let comparable = db.list_track_laps(GameId::Acc, "monza", "Monza").unwrap();
        assert_eq!(comparable.len(), 1);
        assert_eq!(comparable[0].lap_id, lap_id);
        assert_eq!(comparable[0].lap_time_ms, 110_000);
        assert_eq!(comparable[0].player_name, "Dmytro");
    }

    #[test]
    fn two_players_rank_separately() {
        let (dir, db) = open_temp();
        let a = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Alice")
            .unwrap();
        let b = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Bob")
            .unwrap();
        insert_valid_lap(&db, dir.path(), a, 111_000, b"a");
        insert_valid_lap(&db, dir.path(), b, 109_000, b"b");

        let board = db.get_leaderboard(GameId::Acc, "monza", "Monza").unwrap();
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].player_name, "Bob");
        assert_eq!(board[0].rank, 1);
        assert_eq!(board[1].player_name, "Alice");
        assert_eq!(board[1].rank, 2);
    }

    #[test]
    fn player_name_is_case_insensitive() {
        let (dir, db) = open_temp();
        let a = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        let b = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "dmytro")
            .unwrap();
        insert_valid_lap(&db, dir.path(), a, 110_000, b"a");
        insert_valid_lap(&db, dir.path(), b, 108_000, b"b");

        let board = db.get_leaderboard(GameId::Acc, "monza", "Monza").unwrap();
        assert_eq!(board.len(), 2);
        assert_eq!(board[0].lap_time_ms, 108_000);
        assert_eq!(board[0].place, 1);
        assert_eq!(board[0].rank, 1);
        assert_eq!(board[1].lap_time_ms, 110_000);
        assert_eq!(board[1].place, 2);
        assert_eq!(board[1].rank, 2);
    }

    #[test]
    fn backfill_promotes_existing_session_laps() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "spa", "Spa", "Porsche", "1.0", "Alex")
            .unwrap();
        let lap_id = Uuid::new_v4();
        let rel = dummy_parquet(dir.path(), session, lap_id, b"old");
        db.conn
            .execute(
                "INSERT INTO laps (id, session_id, lap_number, lap_time_ms, valid, is_best, is_pinned, sectors_json)
                 VALUES (?1, ?2, 1, 120000, 1, 1, 0, '{}')",
                params![lap_id.to_string(), session.to_string()],
            )
            .unwrap();
        db.conn
            .execute(
                "INSERT INTO lap_files (lap_id, parquet_path, sample_rate_hz, channel_manifest_json)
                 VALUES (?1, ?2, 100, '{}')",
                params![lap_id.to_string(), rel],
            )
            .unwrap();

        assert!(db.get_leaderboard(GameId::Acc, "spa", "Spa").unwrap().is_empty());
        db.backfill_leaderboard_if_empty().unwrap();
        let board = db.get_leaderboard(GameId::Acc, "spa", "Spa").unwrap();
        assert_eq!(board.len(), 1);
        assert_eq!(board[0].lap_time_ms, 120_000);
        assert_eq!(board[0].player_name, "Alex");
    }

    #[test]
    fn fuel_profile_averages_valid_laps() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        insert_fuel_lap(&db, dir.path(), session, 100_000, 2.0, b"a");
        insert_fuel_lap(&db, dir.path(), session, 110_000, 2.4, b"b");

        let rows = db.list_fuel_profiles().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].car, "Ferrari");
        assert_eq!(rows[0].track, "Monza");
        assert_eq!(rows[0].avg_lap_time_ms, Some(105_000));
        let fuel = rows[0].avg_fuel_used_l.expect("fuel average");
        assert!((fuel - 2.2).abs() < 0.001);
    }

    #[test]
    fn deleting_session_keeps_fuel_profile() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "spa", "Spa", "Porsche", "1.0", "Alex")
            .unwrap();
        insert_fuel_lap(&db, dir.path(), session, 120_000, 3.1, b"keep");

        db.delete_session(session, dir.path()).unwrap();
        assert!(db.get_session(session).unwrap().is_none());

        let rows = db.list_fuel_profiles().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].car, "Porsche");
        assert_eq!(rows[0].track, "Spa");
        assert_eq!(rows[0].avg_lap_time_ms, Some(120_000));
        let fuel = rows[0].avg_fuel_used_l.expect("fuel average");
        assert!((fuel - 3.1).abs() < 0.001);
    }

    #[test]
    fn fuel_profiles_keep_deleted_session_in_combined_average() {
        let (dir, db) = open_temp();
        let a = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        let b = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        insert_fuel_lap(&db, dir.path(), a, 100_000, 2.0, b"a");
        insert_fuel_lap(&db, dir.path(), b, 120_000, 4.0, b"b");

        db.delete_session(a, dir.path()).unwrap();

        let rows = db.list_fuel_profiles().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].avg_lap_time_ms, Some(110_000));
        let fuel = rows[0].avg_fuel_used_l.expect("fuel average");
        assert!((fuel - 3.0).abs() < 0.001);
    }

    #[test]
    fn stores_and_orders_laps_by_stint() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        db.insert_lap_with_id(
            first,
            session,
            &summary(110_000, true),
            &dummy_parquet(dir.path(), session, first, b"s1"),
            100.0,
            "{}",
            1,
            None,
        )
        .unwrap();
        db.insert_lap_with_id(
            second,
            session,
            &summary(108_000, true),
            &dummy_parquet(dir.path(), session, second, b"s2"),
            100.0,
            "{}",
            2,
            Some(420),
        )
        .unwrap();

        let laps = db.list_laps(session).unwrap();
        assert_eq!(laps.len(), 2);
        assert_eq!(laps[0].id, first);
        assert_eq!(laps[0].stint, 1);
        assert_eq!(laps[0].stint_break_s, None);
        assert_eq!(laps[1].id, second);
        assert_eq!(laps[1].stint, 2);
        assert_eq!(laps[1].stint_break_s, Some(420));
    }

    #[test]
    fn finalize_drops_a_lapless_session() {
        let (_dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
            .unwrap();

        db.finalize_session(session).unwrap();

        assert!(
            db.list_sessions().unwrap().is_empty(),
            "an empty session should not survive finalize"
        );
        assert!(db.get_session(session).unwrap().is_none());
    }

    #[test]
    fn finalize_keeps_a_session_that_recorded_a_lap() {
        let (dir, db) = open_temp();
        let session = db
            .create_session(GameId::Acc, "spa", "Spa", "Ferrari", "1.0", "Dmytro")
            .unwrap();
        insert_valid_lap(&db, dir.path(), session, 105_000, b"l1");

        db.finalize_session(session).unwrap();

        let sessions = db.list_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, session);
        assert!(sessions[0].ended_at.is_some());
    }

    #[test]
    fn list_sessions_shows_the_live_lapless_session_but_startup_prunes_stale_ones() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("simtelemetry.db");
        let live = {
            let db = Database::open(&db_path).unwrap();
            let live = db
                .create_session(GameId::Acc, "monza", "Monza", "Ferrari", "1.0", "Dmytro")
                .unwrap();
            // Unfinalized + lapless == the session currently recording.
            assert_eq!(db.list_sessions().unwrap().len(), 1);
            live
        };
        // Reopen: the startup sweep treats the now-orphaned lapless session as stale.
        let db = Database::open(&db_path).unwrap();
        assert!(db.list_sessions().unwrap().is_empty());
        assert!(db.get_session(live).unwrap().is_none());
    }
}
