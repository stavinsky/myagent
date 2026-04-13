use crate::config::Config;
use crate::types::Flow;
use anyhow::{Context, Result};
use async_openai::{
    config::OpenAIConfig,
    types::{ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
            ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequestArgs, ChatCompletionTool},
    Client,
};
use serde_json::Value;
use tracing;

pub struct OpenAIClient {
    client: Client<OpenAIConfig>,
    config: Config,
    allowed_base: String,
}

impl OpenAIClient {
    pub fn new(config: Config, allowed_base: String) -> Result<Self> {
        tracing::info!("Initializing OpenAI client with model: {}", config.model);
        
        let api_key = config.get_api_key()?;
        
        let mut client_config = OpenAIConfig::default().with_api_key(api_key);
        
        if let Some(ref base_url) = config.base_url {
            client_config = client_config.with_api_base(base_url);
        }
        
        let client = Client::with_config(client_config);

        Ok(Self { client, config, allowed_base })
    }

    pub async fn execute_flow(&self, flow: &Flow, user_prompt: &str, available_tools: &[String]) -> Result<String> {
        println!("Executing flow: {}", flow.name);
        tracing::info!("Flow description: {}", flow.description);
        tracing::info!("Available tools: {:?}", available_tools);
        tracing::info!("Custom tools: {:?}", self.config.custom_tools.keys());
        
        // Create tool registry with custom tools and get tools
        let registry = crate::tools::registry::ToolRegistry::with_custom_tools(&self.config.custom_tools);
        let tools = registry.get_tools(available_tools);
        
        tracing::debug!("Flow {} initialized with {} tools", flow.name, tools.len());
        
        // Get the combined system prompt (common + flow-specific)
        let combined_system_prompt = self.config.get_combined_system_prompt(flow);
        
        self.execute_with_messages_and_tools(combined_system_prompt, user_prompt.to_string(), &tools, 100, &registry).await
    }

    async fn execute_with_messages_and_tools(&self, system_prompt: String, user_prompt: String, tools: &[ChatCompletionTool], max_iterations: usize, registry: &crate::tools::registry::ToolRegistry) -> Result<String> {
        tracing::debug!("Starting chat completion with {} tools", tools.len());

        let mut messages: Vec<ChatCompletionRequestMessage> = vec![
            ChatCompletionRequestSystemMessageArgs::default()
                .content(system_prompt.as_str())
                .build()?
                .into(),
            ChatCompletionRequestUserMessageArgs::default()
                .content(user_prompt.as_str())
                .build()?
                .into(),
        ];

        for iteration in 0..max_iterations {
            tracing::debug!("Iteration {}/{}", iteration + 1, max_iterations);
            
            let request = CreateChatCompletionRequestArgs::default()
                .model(&self.config.model)
                .messages(messages.clone())
                .tools(tools)
                .build()?;

            tracing::debug!("Sending request to OpenAI");
            let response = self.client
                .chat()
                .create(request)
                .await
                .context("OpenAI API error")?;

            let message = response.choices.first()
                .context("No response from OpenAI")?
                .message
                .clone();

            if let Some(tool_calls) = &message.tool_calls {
                if tool_calls.is_empty() {
                    if let Some(content) = &message.content {
                        return Ok(content.trim().to_string());
                    }
                    return Ok(String::new());
                }
                
                // Reset registry state before processing this batch
                registry.reset_batch();
                
                // Process tool calls and add responses to messages
                self.process_tool_calls(tool_calls, &mut messages, registry)
                    .context("Failed to process tool calls")?;
                
                continue;
            }

            if let Some(content) = &message.content {
                println!("Flow completed");
                println!("{}", content.trim());
                return Ok(content.trim().to_string());
            }
        }

        tracing::error!("Max iterations ({}) reached", max_iterations);
        Err(anyhow::anyhow!("Max iterations reached"))
    }

    /// Process a batch of tool calls and add their responses to the messages.
    /// 
    /// This method iterates through tool calls from the LLM, executes each one using
    /// the tool registry, and appends the results as tool messages to the conversation.
    /// All tool calls in a batch are processed sequentially, with each response being
    /// added to the messages for the next iteration.
    /// 
    /// # Arguments
    /// * `tool_calls` - The batch of tool calls to process
    /// * `messages` - Mutable reference to the messages vector where responses will be added
    /// * `registry` - The tool registry used to execute the tool calls
    /// 
    /// # Returns
    /// * `Ok(())` if all tool calls were processed successfully
    /// * `Err` if any tool call failed to parse or execute
    fn process_tool_calls(
        &self,
        tool_calls: &[async_openai::types::ChatCompletionMessageToolCall],
        messages: &mut Vec<ChatCompletionRequestMessage>,
        registry: &crate::tools::registry::ToolRegistry,
    ) -> Result<()> {
        for tool_call in tool_calls {
            let function = &tool_call.function;
            let arguments: Value = serde_json::from_str(&function.arguments)
                .context("Failed to parse tool arguments")?;
            
            let tool_output = registry.execute_tool(&function.name, &arguments, &self.allowed_base);
            
            messages.push(
                async_openai::types::ChatCompletionRequestToolMessageArgs::default()
                    .content(tool_output)
                    .tool_call_id(tool_call.id.clone())
                    .build()?
                    .into()
            );
        }
        
        Ok(())
    }
}
