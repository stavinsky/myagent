use async_openai::types::chat::{ChatCompletionTool, FunctionObject};
use serde::{Deserialize, Serialize};

use crate::types::{ToolResponse, ToolHandler};
use crate::tui::MultiSelectApp;
use serde_json::Value;

/// Represents a selectable item with optional detail content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectableItem {
    pub id: String,
    pub description: String,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Get the tool definition for the multi_select tool
pub fn get_tool_definition() -> ChatCompletionTool {
    ChatCompletionTool {
        function: FunctionObject {
            name: "multi_select".to_string(),
            description: Some("Present a list of options to the user in a TUI with checkboxes. User can select multiple items using Space to toggle and Enter to confirm.".to_string()),
            parameters: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "question": {
                        "type": "string",
                        "description": "The question or prompt for the user"
                    },
                    "items": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": {"type": "string", "description": "Unique identifier for the item"},
                                "description": {"type": "string", "description": "Short description shown in the list"},
                                "detail": {"type": "string", "description": "Optional detailed information shown when viewing the item"}
                            },
                            "required": ["id", "description"]
                        },
                        "description": "List of selectable items"
                    },
                    "question_type": {
                        "type": "string",
                        "enum": ["multi_select"],
                        "description": "Type of question - always 'multi_select' for this use case"
                    }
                },
                "required": ["question", "items", "question_type"]
            })),
            strict: Some(true),
        },
    }
}

/// Present items in TUI and collect user selection
pub fn execute_multi_select(question: &str, items_json: &str, _question_type: &str) -> ToolResponse {
    tracing::info!("multi_select: {}", question);
    
    // Parse the items from JSON
    let items: Vec<SelectableItem> = match serde_json::from_str(items_json) {
        Ok(i) => i,
        Err(e) => {
            tracing::warn!("multi_select failed: {}", e);
            return ToolResponse::error("multi_select", format!("Failed to parse items: {}", e));
        }
    };
    
    if items.is_empty() {
        tracing::warn!("multi_select: No items provided");
        return ToolResponse::success("multi_select", "NO_ITEMS: No items were provided".to_string());
    }
    
    tracing::debug!("multi_select: {} items available", items.len());
    
    // Launch TUI for multi-select
    let app = MultiSelectApp::new(&items, question);
    let selected_ids = match app.run(question) {
        Ok(ids) => ids,
        Err(e) => {
            tracing::warn!("multi_select TUI failed: {}", e);
            return ToolResponse::error("multi_select", format!("TUI error: {}", e));
        }
    };
    
    if selected_ids.is_empty() {
        tracing::info!("multi_select: No selection made by user");
        return ToolResponse::success("multi_select", "NO_SELECTION: User did not select any items".to_string());
    }
    
    tracing::info!("multi_select: User selected {} items", selected_ids.len());
    
    // Return the selected IDs as a clear format for the model
    let selected_json = match serde_json::to_string(&selected_ids) {
        Ok(json) => json,
        Err(e) => {
            return ToolResponse::error("multi_select", format!("Failed to serialize selection: {}", e));
        }
    };
    
    // Build response with selected items
    let mut response = format!("USER_SELECTION: {}\n", selected_json);
    response.push_str("\nSelected items:\n");
    
    for item in &items {
        if selected_ids.contains(&item.id) {
            response.push_str(&format!("- {}: {}\n", item.id, item.description));
        }
    }
    
    let metadata = format!("{} of {} items selected", selected_ids.len(), items.len());
    ToolResponse::success("multi_select", response).with_metadata(metadata)
}

/// Tool handler implementation for multi_select
pub struct MultiSelectHandler;

impl ToolHandler for MultiSelectHandler {
    fn name(&self) -> &str {
        "multi_select"
    }

    fn get_definition(&self) -> ChatCompletionTool {
        get_tool_definition()
    }

    fn execute(&self, arguments: &Value, _allowed_base: &str) -> ToolResponse {
        let question = match arguments["question"].as_str() {
            Some(s) => s,
            None => return ToolResponse::error("multi_select", "Missing required argument: question".to_string()),
        };
        let items_json = arguments["items"].to_string();
        let question_type = arguments["question_type"].as_str().unwrap_or("multi_select");
        
        execute_multi_select(question, &items_json, question_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selectable_item_creation() {
        let item = SelectableItem {
            id: "item-001".to_string(),
            description: "Security vulnerability".to_string(),
            detail: Some("Add input validation".to_string()),
        };
        
        assert_eq!(item.id, "item-001");
        assert_eq!(item.description, "Security vulnerability");
        assert!(item.detail.is_some());
    }

    #[test]
    fn test_tool_definition() {
        let def = get_tool_definition();
        assert_eq!(def.function.name, "multi_select");
        assert!(def.function.parameters.is_some());
        
        // Verify it expects multi_select question type
        let params = def.function.parameters.unwrap();
        let obj = params.as_object().unwrap();
        assert!(obj.contains_key("required"));
    }
}
