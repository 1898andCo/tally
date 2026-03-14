//! Handler for `tally mcp-capabilities`.

/// Handle `tally mcp-capabilities` — dynamically list all MCP tools, resources, and prompts.
///
/// Instantiates the MCP server to reflect the actual registered tools and prompts,
/// so this output always matches what the server exposes.
pub fn handle_mcp_capabilities() {
    use crate::mcp::server::TallyMcpServer;

    let server = TallyMcpServer::new(".".to_string());

    println!(
        "MCP Capabilities for tally v{}\n",
        env!("CARGO_PKG_VERSION")
    );

    // Tools — reflected from the tool router
    let tools = server.list_tools();
    println!("Tools ({}):", tools.len());
    for tool in &tools {
        let desc = tool.description.as_deref().unwrap_or("(no description)");
        // Truncate description to first sentence for readability
        let short_desc = desc.split(". ").next().unwrap_or(desc);
        println!("  {:<24} {short_desc}", tool.name);
    }

    // Resources — static list (resource templates aren't queryable without RequestContext)
    println!("\nResources (7):");
    println!("  findings://summary              Counts by severity/status + recent");
    println!("  findings://file/{{path}}          All findings in a file");
    println!("  findings://detail/{{uuid}}        Full finding with history, relationships, tags");
    println!("  findings://severity/{{level}}     By severity level");
    println!("  findings://status/{{status}}      By lifecycle state");
    println!("  findings://rule/{{rule_id}}       By rule ID");
    println!("  findings://pr/{{pr_number}}       By PR number");

    // Prompts — reflected from the prompt router
    let prompts = server.list_prompts();
    println!("\nPrompts ({}):", prompts.len());
    for prompt in &prompts {
        let desc = prompt.description.as_deref().unwrap_or("(no description)");
        let short_desc = desc.split(". ").next().unwrap_or(desc);
        println!("  {:<24} {short_desc}", prompt.name);
        if let Some(args) = &prompt.arguments {
            for arg in args {
                let required = if arg.required.unwrap_or(false) {
                    " (required)"
                } else {
                    ""
                };
                println!("    arg: {}{required}", arg.name);
            }
        }
    }

    println!("\nConfigure in .mcp.json:");
    println!("  {{");
    println!("    \"mcpServers\": {{");
    println!("      \"tally\": {{");
    println!("        \"command\": \"tally\",");
    println!("        \"args\": [\"mcp-server\"]");
    println!("      }}");
    println!("    }}");
    println!("  }}");
}
