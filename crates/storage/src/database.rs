use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use sim_core::{
    GameId, LapRecord, LapSummary, LeaderboardEntry, LeaderboardTrackOption, SectorTimes,
    SessionRecord, TrackLapOption,
};
use std::path::Path;
use uuid::Uuid;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path).context("open sqlite database")?;
        conn.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        let db = Self { conn };
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
        Ok(())
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
        )?;
        Ok(id)
    }

    pub fn insert_lap_with_id(
        &self,
        id: Uuid,
        session_id: Uuid,
        summary: &LapSummary,
        parquet_path: &str,
        sample_rate_hz: f32,
        channel_manifest_json: &str,
    ) -> Result<()> {
        let sectors_json = serde_json::to_string(&summary.sectors)?;
        self.conn.execute(
            "INSERT INTO laps (id, session_id, lap_number, lap_time_ms, valid, is_best, is_pinned, sectors_json, tyre_compound, tc_level, abs_level, fuel_used_l) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7, ?8, ?9, ?10)",
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
             FROM sessions s ORDER BY s.started_at DESC",
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
            "SELECT l.id, l.session_id, l.lap_number, l.lap_time_ms, l.valid, l.is_best, l.is_pinned, l.sectors_json, f.sample_rate_hz, l.tyre_compound, l.tc_level, l.abs_level, l.fuel_used_l
             FROM laps l
             JOIN lap_files f ON f.lap_id = l.id
             WHERE l.session_id = ?1
             ORDER BY l.lap_number ASC",
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
            "SELECT DISTINCT s.game FROM sessions s
             INNER JOIN laps l ON l.session_id = s.id
             WHERE l.valid = 1
             ORDER BY s.game",
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
            "SELECT DISTINCT s.track_id, s.track FROM sessions s
             INNER JOIN laps l ON l.session_id = s.id
             WHERE l.valid = 1 AND s.game = ?1
             ORDER BY s.track",
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
            SELECT s.player_name, l.lap_time_ms, l.valid, l.session_id, l.id
            FROM laps l
            INNER JOIN sessions s ON s.id = l.session_id
            WHERE l.valid = 1
              AND s.game = ?1
              AND (
                (?2 != '' AND s.track_id = ?2)
                OR (?2 = '' AND s.track = ?3)
              )
              AND l.lap_time_ms = (
                SELECT MIN(l2.lap_time_ms)
                FROM laps l2
                INNER JOIN sessions s2 ON s2.id = l2.session_id
                WHERE l2.valid = 1
                  AND s2.game = ?1
                  AND (
                    (?2 != '' AND s2.track_id = ?2)
                    OR (?2 = '' AND s2.track = ?3)
                  )
                  AND LOWER(TRIM(s2.player_name)) = LOWER(TRIM(s.player_name))
              )
              AND l.id = (
                SELECT l3.id
                FROM laps l3
                INNER JOIN sessions s3 ON s3.id = l3.session_id
                WHERE l3.valid = 1
                  AND s3.game = ?1
                  AND (
                    (?2 != '' AND s3.track_id = ?2)
                    OR (?2 = '' AND s3.track = ?3)
                  )
                  AND LOWER(TRIM(s3.player_name)) = LOWER(TRIM(s.player_name))
                  AND l3.lap_time_ms = l.lap_time_ms
                ORDER BY l3.lap_time_ms ASC, l3.id ASC
                LIMIT 1
              )
            ORDER BY l.lap_time_ms ASC
            "#,
        )?;
        let rows = stmt.query_map(
            params![game_json, track_id, track_name],
            |row| {
                Ok(LeaderboardEntry {
                    rank: 0,
                    player_name: row.get(0)?,
                    lap_time_ms: row.get(1)?,
                    valid: row.get::<_, i32>(2)? != 0,
                    session_id: Uuid::parse_str(&row.get::<_, String>(3)?)
                        .unwrap_or_default(),
                    lap_id: Uuid::parse_str(&row.get::<_, String>(4)?).unwrap_or_default(),
                })
            },
        )?;
        let mut entries: Vec<LeaderboardEntry> = rows.filter_map(Result::ok).collect();
        for (i, entry) in entries.iter_mut().enumerate() {
            entry.rank = (i + 1) as u32;
        }
        Ok(entries)
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
        let rows = stmt.query_map(
            params![game_json, track_id, track_name],
            |row| {
                let started_at: String = row.get(7)?;
                let sectors_json: String = row.get(8)?;
                Ok(TrackLapOption {
                    lap_id: Uuid::parse_str(&row.get::<_, String>(0)?).unwrap_or_default(),
                    session_id: Uuid::parse_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                    lap_number: row.get(2)?,
                    lap_time_ms: row.get(3)?,
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
            },
        )?;
        Ok(rows.filter_map(Result::ok).collect())
    }
}
