use anyhow::{Context, Result};
use redis::aio::ConnectionManager;

pub async fn connect(url: &str) -> Result<ConnectionManager> {
    let client = redis::Client::open(url).context("open valkey client")?;
    let mgr = ConnectionManager::new(client)
        .await
        .context("connect valkey")?;
    Ok(mgr)
}

pub async fn ping(mgr: &mut ConnectionManager) -> Result<()> {
    let reply: String = redis::cmd("PING").query_async(mgr).await?;
    anyhow::ensure!(reply == "PONG", "unexpected PING reply: {reply}");
    Ok(())
}

pub async fn sadd(mgr: &mut ConnectionManager, key: &str, member: &str) -> Result<()> {
    let _: i64 = redis::cmd("SADD")
        .arg(key)
        .arg(member)
        .query_async(mgr)
        .await?;
    Ok(())
}

pub async fn srem(mgr: &mut ConnectionManager, key: &str, member: &str) -> Result<()> {
    let _: i64 = redis::cmd("SREM")
        .arg(key)
        .arg(member)
        .query_async(mgr)
        .await?;
    Ok(())
}

pub async fn smembers(mgr: &mut ConnectionManager, key: &str) -> Result<Vec<String>> {
    let members: Vec<String> = redis::cmd("SMEMBERS").arg(key).query_async(mgr).await?;
    Ok(members)
}
