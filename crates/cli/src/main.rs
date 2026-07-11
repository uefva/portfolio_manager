//! 小型运维 CLI，用于在不启动桌面客户端的情况下检查 Rust HTTP 服务端状态。
//!
//! 用法示例：
//!   portfolio-cli health         # 检查服务端是否在线
//!   portfolio-cli assets         # 列出所有资产
//!   portfolio-cli holdings       # 查看持仓汇总
//!   portfolio-cli --server http://其他地址:端口 health  # 指定服务端地址

use anyhow::Result;
use clap::{Parser, Subcommand};

/// 命令行选项有意与服务端默认本地 URL 保持一致。
#[derive(Parser)]
struct Cli {
    /// 服务端地址，默认 http://127.0.0.1:8765
    #[arg(long, default_value = "http://127.0.0.1:8765")]
    server: String,

    /// 要执行的子命令
    #[command(subcommand)]
    command: Command,
}

/// 只读请求，安全用于部署诊断。
///
/// 这些命令不会修改数据库，可在生产环境中安全执行。
#[derive(Subcommand)]
enum Command {
    /// 健康检查：确认服务端在线
    Health,
    /// 列出所有已注册资产
    Assets,
    /// 查看当前持仓汇总
    Holdings,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // 端点选择保持显式，以便 CLI 用法本身就能作为 API 文档
    let path = match cli.command {
        Command::Health   => "/api/health",
        Command::Assets   => "/api/portfolio/assets",
        Command::Holdings => "/api/portfolio/holdings",
    };

    // 简单的 GET 请求，输出原始 JSON 到 stdout
    let response = reqwest::get(format!("{}{}", cli.server, path))
        .await?
        .text()
        .await?;
    println!("{response}");
    Ok(())
}
