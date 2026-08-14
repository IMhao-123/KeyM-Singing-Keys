use rusqlite::{params, Connection, OptionalExtension, Transaction};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

pub type DbResult<T> = Result<T, String>;

/// 一次按键事件的原子写入输入（AUD-010）。
pub struct KeyRecord<'a> {
    pub timestamp_ms: i64,
    pub keycode: u16,
    pub category: &'a str,
    pub app_name: Option<&'a str>,
    pub date: &'a str,
    pub hour: u8,
}

type DailyStat = (String, u64, u64);
type Activity = (i64, u16, String, Option<String>);

/// 从时区解析结果中取唯一时刻；歧义（秋季回拨）取较早一侧（AUD-009）
fn pick_earlier<Tz: chrono::TimeZone>(
    result: chrono::LocalResult<chrono::DateTime<Tz>>,
) -> Option<chrono::DateTime<Tz>> {
    match result {
        chrono::LocalResult::Single(v) => Some(v),
        chrono::LocalResult::Ambiguous(a, b) => Some(a.min(b)),
        chrono::LocalResult::None => None,
    }
}

/// 指定时区某本地日期的起点（本地午夜）对应的 UTC 毫秒时间戳（AUD-009）。
fn day_start_ms_in<Tz: chrono::TimeZone>(tz: &Tz, date: chrono::NaiveDate) -> Option<i64> {
    let naive = date.and_hms_opt(0, 0, 0)?;
    if let Some(v) = pick_earlier(tz.from_local_datetime(&naive)) {
        return Some(v.timestamp_millis());
    }
    (1..=180).find_map(|minutes| {
        pick_earlier(tz.from_local_datetime(&(naive + chrono::Duration::minutes(minutes))))
            .map(|v| v.timestamp_millis())
    })
}

/// 本机时区的今日起点（UTC 毫秒）。不可达失败退化为旧行为，绝不 panic（AUD-009）。
fn local_today_start_ms() -> i64 {
    let today = chrono::Local::now().date_naive();
    day_start_ms_in(&chrono::Local, today).unwrap_or_else(|| {
        today
            .and_hms_opt(0, 0, 0)
            .expect("有效日期的午夜必然可构造")
            .and_utc()
            .timestamp_millis()
    })
}

pub struct Database {
    conn: Mutex<Connection>,
}

impl Database {
    /// 生产构造器。先安全迁移旧库，再打开新库（AUD-008）。
    pub fn new() -> DbResult<Self> {
        let data = crate::data_paths::data_root()?;
        let new_db = data.join("KeyM/stats.db");
        let old_db = data.join("MacKeyboard/stats.db");
        Self::migrate_database(&old_db, &new_db)?;
        Self::open_at(new_db)
    }

    /// 显式路径构造器（测试与诊断用，禁止指向真实用户目录之外的隐式位置）。
    /// 旧库兼容：第一版裁掉 WPM 后，新建库不再创建 wpm_samples 表与
    /// daily_stats.peak_wpm 列；旧库中已存在的同名表/列原样保留、不再读写，
    /// `CREATE TABLE IF NOT EXISTS` 语义保证打开旧库不会失败。
    pub fn open_at(path: impl AsRef<Path>) -> DbResult<Self> {
        let db_path = path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("创建数据库目录失败: {e}"))?;
        }
        let conn = Connection::open(&db_path).map_err(|e| format!("无法打开数据库: {e}"))?;
        Self::configure_and_initialize(&conn)?;
        let db = Self {
            conn: Mutex::new(conn),
        };
        db.backfill_if_needed()?;
        Ok(db)
    }

    fn configure_and_initialize(conn: &Connection) -> DbResult<()> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")
            .map_err(|e| format!("数据库 pragma 设置失败: {e}"))?;
        conn.execute_batch(SCHEMA)
            .map_err(|e| format!("数据库初始化失败: {e}"))
    }

    /// AUD-008：旧 WAL 库安全迁移。用 `VACUUM INTO` 得到包含已提交 WAL 页的一致快照，
    /// 写临时文件后做 integrity_check 与关键表行数比对，成功后原子改名。
    /// 只在新库不存在且旧库存在时执行；禁止对真实旧库试验。
    pub fn migrate_database(old_db: &Path, new_db: &Path) -> DbResult<bool> {
        if new_db.exists() || !old_db.exists() {
            return Ok(false);
        }
        let parent = new_db
            .parent()
            .ok_or_else(|| "新数据库路径没有父目录".to_string())?;
        std::fs::create_dir_all(parent).map_err(|e| format!("创建迁移目录失败: {e}"))?;
        let tmp = parent.join(format!(".stats.db.migrate-{}", std::process::id()));
        if tmp.exists() {
            std::fs::remove_file(&tmp).map_err(|e| e.to_string())?;
        }

        let source = Connection::open(old_db).map_err(|e| format!("打开旧数据库失败: {e}"))?;
        // VACUUM INTO 经 SQLite 读取，仍只在 -wal 中的已提交页会被包含。
        source
            .execute("VACUUM INTO ?1", params![tmp.to_string_lossy().as_ref()])
            .map_err(|e| format!("创建迁移快照失败: {e}"))?;
        let snapshot = Connection::open(&tmp).map_err(|e| format!("打开迁移快照失败: {e}"))?;
        let integrity: String = snapshot
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| format!("迁移完整性检查失败: {e}"))?;
        if integrity != "ok" {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("迁移完整性检查失败: {integrity}"));
        }
        // 只比对第一版仍保留的核心表；旧库可能存在的 wpm_samples 属已裁功能，不校验。
        for table in [
            "key_events",
            "click_events",
            "daily_stats",
            "hourly_distribution",
        ] {
            let source_count = table_count_if_present(&source, table)?;
            let snapshot_count = table_count_if_present(&snapshot, table)?;
            if source_count != snapshot_count {
                let _ = std::fs::remove_file(&tmp);
                return Err(format!("迁移行数不一致: {table}"));
            }
        }
        drop(snapshot);
        std::fs::rename(&tmp, new_db).map_err(|e| format!("替换迁移数据库失败: {e}"))?;
        log::info!("旧库已通过 VACUUM INTO 安全迁移到新库");
        Ok(true)
    }

    fn lock(&self) -> DbResult<MutexGuard<'_, Connection>> {
        self.conn.lock().map_err(|_| "数据库锁已损坏".to_string())
    }

    /// AUD-010：一次按键的原始记录 + 日聚合 + 小时聚合在同一事务中原子完成。
    pub fn record_key_transaction(&self, record: KeyRecord<'_>) -> DbResult<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO key_events(timestamp,keycode,category,app_name) VALUES(?1,?2,?3,?4)",
            params![
                record.timestamp_ms,
                record.keycode,
                record.category,
                record.app_name
            ],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO daily_stats(date,total_keys) VALUES(?1,1) ON CONFLICT(date) DO UPDATE SET total_keys=total_keys+1",
            [record.date],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO hourly_distribution(date,hour,key_count) VALUES(?1,?2,1) ON CONFLICT(date,hour) DO UPDATE SET key_count=key_count+1",
            params![record.date, record.hour],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)
    }

    /// AUD-010：一次点击的原始记录 + 日聚合在同一事务中原子完成。
    pub fn record_click_transaction(
        &self,
        timestamp_ms: i64,
        button: &str,
        app_name: Option<&str>,
        date: &str,
    ) -> DbResult<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        tx.execute(
            "INSERT INTO click_events(timestamp,button,app_name) VALUES(?1,?2,?3)",
            params![timestamp_ms, button, app_name],
        )
        .map_err(db_err)?;
        tx.execute(
            "INSERT INTO daily_stats(date,total_clicks) VALUES(?1,1) ON CONFLICT(date) DO UPDATE SET total_clicks=total_clicks+1",
            [date],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)
    }

    // AUD-021：所有读取返回 Result，区分“真实空数据”与“数据库读取失败”。
    pub fn try_get_total_keys(&self) -> DbResult<u64> {
        self.lock()?
            .query_row("SELECT COUNT(*) FROM key_events", [], |r| r.get(0))
            .map_err(db_err)
    }

    pub fn try_get_total_clicks(&self) -> DbResult<u64> {
        self.lock()?
            .query_row("SELECT COUNT(*) FROM click_events", [], |r| r.get(0))
            .map_err(db_err)
    }

    pub fn try_get_today_keys(&self) -> DbResult<u64> {
        let ts = local_today_start_ms();
        self.lock()?
            .query_row(
                "SELECT COUNT(*) FROM key_events WHERE timestamp>=?1",
                [ts],
                |r| r.get(0),
            )
            .map_err(db_err)
    }

    pub fn try_get_app_stats_today(&self) -> DbResult<Vec<(String, u64)>> {
        let ts = local_today_start_ms();
        query_collect(
            &*self.lock()?,
            "SELECT COALESCE(app_name,'Unknown'),COUNT(*) FROM key_events WHERE timestamp>=?1 GROUP BY app_name ORDER BY COUNT(*) DESC LIMIT 20",
            params![ts],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    pub fn try_get_keycode_stats(&self) -> DbResult<Vec<(u16, u64)>> {
        query_collect(
            &*self.lock()?,
            "SELECT keycode,COUNT(*) FROM key_events GROUP BY keycode ORDER BY COUNT(*) DESC",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    /// 导出为 CSV（CSV 导出属待定功能，沿用既有拼接实现，仅改为返回 Result）。
    pub fn export_csv(&self) -> DbResult<String> {
        let conn = self.lock()?;
        let mut stmt = conn
            .prepare(
                "SELECT timestamp,keycode,category,app_name FROM key_events ORDER BY timestamp",
            )
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(format!(
                    "{},{},{},{}",
                    r.get::<_, i64>(0)?,
                    r.get::<_, u16>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?.unwrap_or_default()
                ))
            })
            .map_err(db_err)?;
        let mut csv = String::from("timestamp,keycode,category,app_name\n");
        for row in rows {
            csv.push_str(&row.map_err(db_err)?);
            csv.push('\n');
        }
        Ok(csv)
    }

    /// 导出为 JSON
    pub fn export_json(&self) -> DbResult<String> {
        let rows = self.try_get_recent_activity(u32::MAX)?;
        let events: Vec<_> = rows
            .into_iter()
            .rev()
            .map(|(timestamp, keycode, category, app_name)| {
                serde_json::json!({"timestamp":timestamp,"keycode":keycode,"category":category,"app_name":app_name})
            })
            .collect();
        serde_json::to_string_pretty(&events).map_err(|e| e.to_string())
    }

    // ===== 聚合查询（AUD-021：返回 Result，逐行错误使整次查询失败） =====

    pub fn try_get_daily_stats_range(&self, start: &str, end: &str) -> DbResult<Vec<DailyStat>> {
        query_collect(
            &*self.lock()?,
            "SELECT date,total_keys,total_clicks FROM daily_stats WHERE date>=?1 AND date<=?2 ORDER BY date",
            params![start, end],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
    }

    pub fn try_get_hourly_distribution(&self, date: &str) -> DbResult<Vec<(u8, u64)>> {
        query_collect(
            &*self.lock()?,
            "SELECT hour,key_count FROM hourly_distribution WHERE date=?1 ORDER BY hour",
            [date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }

    /// 热力图：某时间范围内的 keycode 计数（period: today/week/month/all）
    pub fn try_get_heatmap_data(&self, period: &str) -> DbResult<Vec<(u16, u64)>> {
        let since_ts: Option<i64> = match period {
            "today" => Some(local_today_start_ms()),
            "week" => Some((chrono::Local::now() - chrono::Duration::days(7)).timestamp_millis()),
            "month" => Some((chrono::Local::now() - chrono::Duration::days(30)).timestamp_millis()),
            "all" => None,
            _ => return Err(format!("未知热力图周期: {period}")),
        };
        if let Some(ts) = since_ts {
            query_collect(
                &*self.lock()?,
                "SELECT keycode,COUNT(*) FROM key_events WHERE timestamp>=?1 GROUP BY keycode ORDER BY COUNT(*) DESC",
                params![ts],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        } else {
            query_collect(
                &*self.lock()?,
                "SELECT keycode,COUNT(*) FROM key_events GROUP BY keycode ORDER BY COUNT(*) DESC",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
        }
    }

    pub fn try_get_recent_activity(&self, limit: u32) -> DbResult<Vec<Activity>> {
        query_collect(
            &*self.lock()?,
            "SELECT timestamp,keycode,category,app_name FROM key_events ORDER BY timestamp DESC LIMIT ?1",
            params![limit],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
    }

    /// 清空第一版保留的 4 张表（事务化）。
    /// 旧库中可能存在的 wpm_samples 表属已裁掉的 WPM 功能，按数据保护红线不删除。
    pub fn clear_all_data(&self) -> DbResult<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        for table in [
            "key_events",
            "click_events",
            "daily_stats",
            "hourly_distribution",
        ] {
            tx.execute(&format!("DELETE FROM {table}"), [])
                .map_err(db_err)?;
        }
        tx.commit().map_err(db_err)
    }

    /// AUD-011：监听启动前的同步、事务化、幂等回填。
    /// 使用 `app_meta` 完成标记保证只执行一次；`ON CONFLICT` 保证与实时写入并发时精确一次。
    pub fn backfill_aggregates(&self) -> DbResult<()> {
        let mut conn = self.lock()?;
        let tx = conn.transaction().map_err(db_err)?;
        backfill(&tx)?;
        tx.commit().map_err(db_err)
    }

    fn backfill_if_needed(&self) -> DbResult<bool> {
        let mut conn = self.lock()?;
        let completed: Option<String> = conn
            .query_row(
                "SELECT value FROM app_meta WHERE key='aggregate_backfill_v1'",
                [],
                |row| row.get(0),
            )
            .optional()
            .map_err(db_err)?;
        if completed.as_deref() == Some("complete") {
            return Ok(false);
        }
        let tx = conn.transaction().map_err(db_err)?;
        backfill(&tx)?;
        tx.execute(
            "INSERT INTO app_meta(key,value) VALUES('aggregate_backfill_v1','complete') ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            [],
        )
        .map_err(db_err)?;
        tx.commit().map_err(db_err)?;
        Ok(true)
    }
}

/// AUD-011：幂等回填。所有 INSERT...SELECT 使用 ON CONFLICT，重复执行结果一致。
fn backfill(tx: &Transaction<'_>) -> DbResult<()> {
    tx.execute(
        "INSERT INTO daily_stats(date,total_keys) SELECT date(timestamp/1000,'unixepoch','localtime'),COUNT(*) FROM key_events GROUP BY 1 ON CONFLICT(date) DO UPDATE SET total_keys=excluded.total_keys",
        [],
    )
    .map_err(db_err)?;
    tx.execute(
        "INSERT INTO daily_stats(date,total_clicks) SELECT date(timestamp/1000,'unixepoch','localtime'),COUNT(*) FROM click_events GROUP BY 1 ON CONFLICT(date) DO UPDATE SET total_clicks=excluded.total_clicks",
        [],
    )
    .map_err(db_err)?;
    tx.execute(
        "INSERT INTO hourly_distribution(date,hour,key_count) SELECT date(timestamp/1000,'unixepoch','localtime'),CAST(strftime('%H',timestamp/1000,'unixepoch','localtime') AS INTEGER),COUNT(*) FROM key_events GROUP BY 1,2 ON CONFLICT(date,hour) DO UPDATE SET key_count=excluded.key_count",
        [],
    )
    .map_err(db_err)?;
    Ok(())
}

fn db_err(e: rusqlite::Error) -> String {
    e.to_string()
}

fn query_collect<T, P, F>(conn: &Connection, sql: &str, params: P, mut f: F) -> DbResult<Vec<T>>
where
    P: rusqlite::Params,
    F: FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
{
    let mut stmt = conn.prepare(sql).map_err(db_err)?;
    let rows = stmt.query_map(params, |r| f(r)).map_err(db_err)?;
    rows.collect::<rusqlite::Result<Vec<_>>>().map_err(db_err)
}

fn table_count_if_present(conn: &Connection, table: &str) -> DbResult<u64> {
    let exists: Option<u8> = conn
        .query_row(
            "SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1",
            [table],
            |r| r.get(0),
        )
        .optional()
        .map_err(db_err)?;
    if exists.is_none() {
        return Ok(0);
    }
    conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .map_err(db_err)
}

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS key_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    keycode INTEGER NOT NULL,
    category TEXT NOT NULL,
    app_name TEXT
);
CREATE TABLE IF NOT EXISTS click_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp INTEGER NOT NULL,
    button TEXT NOT NULL,
    app_name TEXT
);
CREATE INDEX IF NOT EXISTS idx_key_events_ts ON key_events(timestamp);
CREATE INDEX IF NOT EXISTS idx_click_events_ts ON click_events(timestamp);

CREATE TABLE IF NOT EXISTS daily_stats (
    date TEXT PRIMARY KEY,
    total_keys INTEGER NOT NULL DEFAULT 0,
    total_clicks INTEGER NOT NULL DEFAULT 0,
    active_seconds INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS hourly_distribution (
    date TEXT NOT NULL,
    hour INTEGER NOT NULL,
    key_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (date, hour)
);
CREATE TABLE IF NOT EXISTS app_meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
"#;

#[cfg(test)]
mod tests {
    use super::*;

    // ---- AUD-009：本地日期边界（确定性，不依赖墙钟与宿主时区） ----

    #[test]
    fn utc8_local_midnight_is_previous_utc_day_16h() {
        let ms = day_start_ms_in(
            &chrono::FixedOffset::east_opt(8 * 3600).unwrap(),
            date(2026, 8, 13),
        )
        .expect("UTC+8 午夜必须可解析");
        assert_eq!(ms, utc_ms(2026, 8, 12, 16, 0));
    }

    #[test]
    fn utc_midnight_unchanged() {
        let ms = day_start_ms_in(&chrono::Utc, date(2026, 8, 13)).unwrap();
        assert_eq!(ms, utc_ms(2026, 8, 13, 0, 0));
    }

    #[test]
    fn gap_midnight_nudges_to_first_valid_local_time() {
        let ms = day_start_ms_in(&chrono_tz::America::Santiago, date(2024, 9, 8))
            .expect("间隙日必须向后推移而不是失败");
        assert_eq!(ms, utc_ms(2024, 9, 8, 4, 0));
    }

    #[test]
    fn ambiguous_local_time_prefers_earlier_occurrence() {
        use chrono::TimeZone;
        let tz = chrono_tz::America::New_York;
        let naive = date(2024, 11, 3).and_hms_opt(1, 30, 0).unwrap();
        let resolved = pick_earlier(tz.from_local_datetime(&naive)).expect("歧义时刻必须有解");
        assert_eq!(resolved.timestamp_millis(), utc_ms(2024, 11, 3, 5, 30));
    }

    #[test]
    fn fully_skipped_day_returns_none() {
        assert!(day_start_ms_in(&chrono_tz::Pacific::Apia, date(2011, 12, 30)).is_none());
    }

    #[test]
    fn local_today_start_roundtrips_to_local_early_morning() {
        use chrono::Timelike;
        let today = chrono::Local::now().date_naive();
        let ms = local_today_start_ms();
        let back = chrono::DateTime::from_timestamp_millis(ms)
            .unwrap()
            .with_timezone(&chrono::Local);
        assert_eq!(back.date_naive(), today);
        assert!(back.hour() < 2, "今日起点必须是本地凌晨，实际 {back}");
    }

    fn date(y: i32, m: u32, d: u32) -> chrono::NaiveDate {
        chrono::NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn utc_ms(y: i32, m: u32, d: u32, hh: u32, mm: u32) -> i64 {
        date(y, m, d)
            .and_hms_opt(hh, mm, 0)
            .unwrap()
            .and_utc()
            .timestamp_millis()
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!(
            "keym-db-{}-{}-{}",
            tag,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn open_temp(tag: &str) -> (std::path::PathBuf, Database) {
        let dir = temp_dir(tag);
        let path = dir.join("stats.db");
        let db = Database::open_at(&path).expect("临时库创建失败");
        (path, db)
    }

    // ---- AUD-010/021：事务写入与读取链路 ----

    #[test]
    fn record_key_transaction_writes_atomically_and_reads_back() {
        let (path, db) = open_temp("roundtrip");
        let now = chrono::Utc::now().timestamp_millis();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        db.record_key_transaction(KeyRecord {
            timestamp_ms: now,
            keycode: 0x00,
            category: "normal",
            app_name: Some("TestApp"),
            date: &today,
            hour: 10,
        })
        .unwrap();
        db.record_click_transaction(now, "left", Some("TestApp"), &today)
            .unwrap();

        assert_eq!(db.try_get_total_keys().unwrap(), 1);
        assert_eq!(db.try_get_total_clicks().unwrap(), 1);
        assert_eq!(db.try_get_today_keys().unwrap(), 1);

        let range = db.try_get_daily_stats_range(&today, &today).unwrap();
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].0, today);
        assert_eq!(range[0].1, 1); // total_keys
        assert_eq!(range[0].2, 1); // total_clicks

        let hourly = db.try_get_hourly_distribution(&today).unwrap();
        assert_eq!(hourly, vec![(10u8, 1u64)]);

        let apps = db.try_get_app_stats_today().unwrap();
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].0, "TestApp");

        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// AUD-010：事务中途失败时所有相关表都不变化。
    #[test]
    fn transaction_rolls_back_all_related_tables() {
        let (path, db) = open_temp("tx-rollback");
        {
            let conn = db.lock().unwrap();
            conn.execute(
                "CREATE TRIGGER fail_hour BEFORE INSERT ON hourly_distribution BEGIN SELECT RAISE(ABORT,'injected'); END",
                [],
            )
            .unwrap();
        }
        let result = db.record_key_transaction(KeyRecord {
            timestamp_ms: 1,
            keycode: 2,
            category: "normal",
            app_name: None,
            date: "2026-01-01",
            hour: 1,
        });
        assert!(result.is_err(), "注入失败后写入必须返回错误");
        // 事务回滚：原始记录与日聚合都不应留下
        assert_eq!(db.try_get_total_keys().unwrap(), 0);
        assert!(db
            .try_get_daily_stats_range("2026-01-01", "2026-01-01")
            .unwrap()
            .is_empty());
        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// AUD-021：损坏 schema 的读取返回错误，不能伪装成 0 或空列表。
    #[test]
    fn read_errors_propagate_instead_of_faking_empty() {
        let (path, db) = open_temp("read-err");
        {
            let conn = db.lock().unwrap();
            // 删除表模拟 schema 损坏
            conn.execute_batch("DROP TABLE key_events;").unwrap();
        }
        assert!(db.try_get_total_keys().is_err());
        assert!(db.try_get_today_keys().is_err());
        assert!(db.try_get_keycode_stats().is_err());
        assert!(db.try_get_app_stats_today().is_err());
        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// AUD-008：旧 WAL 库迁移保留已提交 WAL 页。
    #[test]
    fn wal_migration_keeps_committed_rows() {
        let dir = temp_dir("wal");
        let old = dir.join("old/stats.db");
        let new = dir.join("new/stats.db");
        std::fs::create_dir_all(old.parent().unwrap()).unwrap();
        {
            let c = Connection::open(&old).unwrap();
            c.execute_batch(SCHEMA).unwrap();
            c.execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
                .unwrap();
            c.execute(
                "INSERT INTO key_events(timestamp,keycode,category) VALUES(1,2,'normal')",
                [],
            )
            .unwrap();
            // 不 checkpoint：已提交页仍只在 -wal 中
            assert!(old.with_extension("db-wal").exists());
            assert!(Database::migrate_database(&old, &new).unwrap());
        }
        let db = Database::open_at(&new).unwrap();
        assert_eq!(db.try_get_total_keys().unwrap(), 1);
        drop(db);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// AUD-008：新库已存在时不迁移。
    #[test]
    fn migration_skipped_when_target_exists() {
        let dir = temp_dir("mig-skip");
        let old = dir.join("old.db");
        let new = dir.join("new.db");
        Connection::open(&old).unwrap();
        std::fs::write(&new, b"already-here").unwrap();
        assert!(!Database::migrate_database(&old, &new).unwrap());
        assert_eq!(std::fs::read(&new).unwrap(), b"already-here");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// AUD-011：回填幂等且与已存在的当日行正确合并。
    #[test]
    fn backfill_is_idempotent_and_merges_existing_day() {
        let (path, db) = open_temp("fill");
        {
            let c = db.lock().unwrap();
            c.execute(
                "INSERT INTO key_events(timestamp,keycode,category) VALUES(1767225600000,1,'x')",
                [],
            )
            .unwrap();
            c.execute(
                "INSERT INTO daily_stats(date,total_keys) VALUES('2026-01-01',99)",
                [],
            )
            .unwrap();
        }
        db.backfill_aggregates().unwrap();
        let once = db
            .try_get_daily_stats_range("2026-01-01", "2026-01-01")
            .unwrap();
        db.backfill_aggregates().unwrap();
        assert_eq!(
            once,
            db.try_get_daily_stats_range("2026-01-01", "2026-01-01")
                .unwrap()
        );
        // 回填用 key_events 实际行数覆盖：1 行 -> total_keys=1
        assert_eq!(once[0].1, 1);
        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// AUD-011：启动回填只执行一次并持久化完成标记。
    #[test]
    fn startup_backfill_runs_once_and_persists_marker() {
        let dir = temp_dir("fill-marker");
        let path = dir.join("stats.db");
        // 首次打开触发回填
        let db = Database::open_at(&path).unwrap();
        assert!(
            !db.backfill_if_needed().unwrap(),
            "标记已存在，不应再次回填"
        );
        let marker: String = db
            .lock()
            .unwrap()
            .query_row(
                "SELECT value FROM app_meta WHERE key='aggregate_backfill_v1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(marker, "complete");
        drop(db);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn export_csv_json_work() {
        let (path, db) = open_temp("export");
        let now = chrono::Utc::now().timestamp_millis();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        db.record_key_transaction(KeyRecord {
            timestamp_ms: now,
            keycode: 0x00,
            category: "normal",
            app_name: Some("TestApp"),
            date: &today,
            hour: 10,
        })
        .unwrap();

        let csv = db.export_csv().expect("CSV 导出失败");
        assert!(csv.starts_with("timestamp,keycode,category,app_name\n"));
        assert!(csv.contains(",0,normal,TestApp"));

        let json = db.export_json().expect("JSON 导出失败");
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v.as_array().unwrap().len(), 1);
        assert_eq!(v[0]["keycode"], 0);
        assert_eq!(v[0]["app_name"], "TestApp");

        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn clear_all_data_empties_core_tables() {
        let (path, db) = open_temp("clear");
        let now = chrono::Utc::now().timestamp_millis();
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        db.record_key_transaction(KeyRecord {
            timestamp_ms: now,
            keycode: 0x00,
            category: "normal",
            app_name: None,
            date: &today,
            hour: 10,
        })
        .unwrap();

        db.clear_all_data().expect("清空数据失败");
        assert_eq!(db.try_get_total_keys().unwrap(), 0);
        assert!(db
            .try_get_daily_stats_range("0000-01-01", "9999-12-31")
            .unwrap()
            .is_empty());

        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn new_schema_has_no_wpm_table() {
        let (path, db) = open_temp("schema");
        {
            let conn = db.lock().unwrap();
            let count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wpm_samples'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(count, 0, "新库不应再创建 wpm_samples 表");
            let mut stmt = conn.prepare("PRAGMA table_info(daily_stats)").unwrap();
            let cols: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .unwrap()
                .filter_map(|r| r.ok())
                .collect();
            assert!(
                !cols.iter().any(|c| c == "peak_wpm"),
                "新库 daily_stats 不应再有 peak_wpm 列"
            );
        }
        drop(db);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    /// 旧库兼容：含 peak_wpm 列与 wpm_samples 表的旧库打开不失败、旧数据原样保留。
    #[test]
    fn legacy_db_with_wpm_data_opens_and_preserves_data() {
        let dir = temp_dir("legacy");
        let path = dir.join("stats.db");

        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE key_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp INTEGER NOT NULL,
                    keycode INTEGER NOT NULL,
                    category TEXT NOT NULL,
                    app_name TEXT
                );
                CREATE TABLE click_events (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp INTEGER NOT NULL,
                    button TEXT NOT NULL,
                    app_name TEXT
                );
                CREATE TABLE daily_stats (
                    date TEXT PRIMARY KEY,
                    total_keys INTEGER NOT NULL DEFAULT 0,
                    total_clicks INTEGER NOT NULL DEFAULT 0,
                    peak_wpm REAL NOT NULL DEFAULT 0,
                    active_seconds INTEGER NOT NULL DEFAULT 0
                );
                CREATE TABLE hourly_distribution (
                    date TEXT NOT NULL,
                    hour INTEGER NOT NULL,
                    key_count INTEGER NOT NULL DEFAULT 0,
                    PRIMARY KEY (date, hour)
                );
                CREATE TABLE wpm_samples (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    timestamp INTEGER NOT NULL,
                    date TEXT NOT NULL,
                    wpm REAL NOT NULL
                );
                INSERT INTO daily_stats (date, total_keys, total_clicks, peak_wpm)
                    VALUES ('2026-08-01', 1234, 56, 78.5);
                INSERT INTO wpm_samples (timestamp, date, wpm)
                    VALUES (1754000000000, '2026-08-01', 78.5);
                "#,
            )
            .unwrap();
        }

        let db = Database::open_at(&path).expect("旧库打开失败");

        let range = db
            .try_get_daily_stats_range("2026-08-01", "2026-08-01")
            .unwrap();
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].1, 1234);
        assert_eq!(range[0].2, 56);

        {
            let conn = db.lock().unwrap();
            let wpm_rows: i64 = conn
                .query_row("SELECT COUNT(*) FROM wpm_samples", [], |row| row.get(0))
                .unwrap();
            assert_eq!(wpm_rows, 1, "旧 wpm_samples 数据必须保留");
            let peak: f64 = conn
                .query_row(
                    "SELECT peak_wpm FROM daily_stats WHERE date='2026-08-01'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            assert!((peak - 78.5).abs() < f64::EPSILON, "旧 peak_wpm 必须保留");
        }

        // 新写入在旧库上正常工作
        db.record_key_transaction(KeyRecord {
            timestamp_ms: chrono::Utc::now().timestamp_millis(),
            keycode: 0x00,
            category: "normal",
            app_name: None,
            date: "2026-08-12",
            hour: 1,
        })
        .unwrap();
        let range2 = db
            .try_get_daily_stats_range("2026-08-12", "2026-08-12")
            .unwrap();
        assert_eq!(range2.len(), 1);
        assert_eq!(range2[0].1, 1);

        drop(db);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
