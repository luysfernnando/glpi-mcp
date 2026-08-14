use std::sync::Arc;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{ServerCapabilities, ServerInfo};
use rmcp::{ServerHandler, tool_handler};

use crate::client::GLPIClient;
use crate::labels::Labels;

const INSTRUCTIONS: &str = "IMPORTANT: All text content fields sent to GLPI (such as \
    'content', 'answer', 'name', etc.) MUST be formatted in GLPI-compatible HTML. Never use \
    Markdown syntax. Use HTML tags instead: <p> for paragraphs, <strong> for bold, <em> for \
    italic, <ul>/<li> for bullet lists, <ol>/<li> for numbered lists, <h1>-<h3> for headings, \
    <code> for inline code, <pre> for code blocks, <br> for line breaks. Do NOT use #, **, *, \
    ``` or any other Markdown syntax in content fields.";

/// MCP server exposing GLPI as tools. Each `tools/*.rs` module contributes its own
/// `#[tool_router]` impl block for this struct; `new` merges them with `+`, so adding a
/// tool category is a one-line addition here plus a new file under `tools/`.
#[derive(Clone)]
pub struct GlpiServer {
    pub(crate) client: Arc<GLPIClient>,
    pub(crate) labels: Arc<Labels>,
    tool_router: ToolRouter<Self>,
}

impl GlpiServer {
    pub fn new(client: Arc<GLPIClient>, labels: Arc<Labels>) -> Self {
        Self {
            client,
            labels,
            tool_router: Self::tickets_tool_router()
                + Self::session_tool_router()
                + Self::users_groups_tool_router()
                + Self::followups_tool_router()
                + Self::tasks_tool_router()
                + Self::solutions_tool_router()
                + Self::categories_tool_router()
                + Self::stats_tool_router()
                + Self::kb_tool_router()
                + Self::profiles_tool_router()
                + Self::rules_tool_router(),
        }
    }
}

#[tool_handler(router = self.tool_router)]
impl ServerHandler for GlpiServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build())
            .with_instructions(INSTRUCTIONS)
    }
}
