//! Desktop 和 Web 入口点。
//!
//! 两个平台渲染相同的 Dioxus 组件树。
//! `dioxus::launch` 会根据编译时特性自动选择 Desktop（WebView）或 Web（WASM）平台。

mod api;    // HTTP 客户端：封装对服务端的 GET/POST 请求
mod app;    // 应用外壳：标签页导航、全局信号、启动时自动加载
mod views;  // 各页面组件：持仓、资产管理、交易记录、历史快照、收益走势

fn main() {
    // 启动 Dioxus 渲染引擎，根组件为 App
    dioxus::launch(app::App);
}
