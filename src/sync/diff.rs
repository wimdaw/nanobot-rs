use super::assets::AssetSyncer;
use super::tracker::UpstreamTracker;
use anyhow::Result;

pub struct DiffReporter;

impl DiffReporter {
    pub async fn generate_report() -> Result<String> {
        let tracker = UpstreamTracker::new();
        let syncer = AssetSyncer::new();
        let state = syncer.load_state();

        let commits = tracker.fetch_latest_commits(10).await?;
        let latest_tag = tracker.fetch_latest_tag().await?;

        let mut report = String::new();
        report.push_str("====================================================\n");
        report.push_str("   🔄 HKUDS/nanobot 上游版本同步检测报告\n");
        report.push_str("====================================================\n\n");

        report.push_str(&format!("* 上游仓库: https://github.com/{}\n", state.upstream_repo));
        report.push_str(&format!("* 监控分支: {}\n", state.tracked_branch));
        report.push_str(&format!("* 本地记录上次同步 Commit: {}\n", if state.last_synced_commit.is_empty() { "无 (首发版本)" } else { &state.last_synced_commit }));
        report.push_str(&format!("* 最新发布 Release Tag: {}\n\n", latest_tag.unwrap_or_else(|| "none".to_string())));

        report.push_str("【上游最新 10 次提交记录】:\n");
        for (i, c) in commits.iter().enumerate() {
            let short_sha = if c.sha.len() > 7 { &c.sha[..7] } else { &c.sha };
            let first_line = c.message.lines().next().unwrap_or("");
            report.push_str(&format!(" {}. [{}] {} (by {}, {})\n", i + 1, short_sha, first_line, c.author, &c.date[..10]));
        }

        report.push_str("\n【已同步嵌入的静态 Prompt 模板】:\n");
        for f in &state.synced_files {
            report.push_str(&format!(" - {}\n", f));
        }

        report.push_str("\n【建议】: 可运行 `nanobot sync pull-assets` 自动更新模板文件，或查看 Diff 适配逻辑变更。\n");
        Ok(report)
    }
}
