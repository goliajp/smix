use anyhow::Result;
use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn connect(url: &str) -> Result<PgPool> {
    let pool = PgPoolOptions::new()
        .max_connections(16)
        .connect(url)
        .await?;
    Ok(pool)
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    // Serialize DDL with a session advisory lock: Postgres `CREATE TABLE IF
    // NOT EXISTS` is not safe under concurrent execution (racing inserts into
    // the pg_type catalog hit a unique-constraint violation). The lock is on a
    // dedicated connection so a single migrating process is guaranteed.
    let mut conn = pool.acquire().await?;
    sqlx::query("SELECT pg_advisory_lock($1)")
        .bind(0x736d_785f_6d69_6772_i64) // "smx_migr"
        .execute(&mut *conn)
        .await?;
    let result = async {
        for sql in [include_str!("../migrations/0001_init.sql")] {
            sqlx::raw_sql(sql).execute(&mut *conn).await?;
        }
        Ok::<(), sqlx::Error>(())
    }
    .await;
    sqlx::query("SELECT pg_advisory_unlock($1)")
        .bind(0x736d_785f_6d69_6772_i64)
        .execute(&mut *conn)
        .await?;
    result?;
    Ok(())
}
