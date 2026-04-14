# Plan: Rust CLI Coding Agent with TUI

Build `myagent` — a Rust CLI tool that executes configurable workflows (flows) using OpenAI's API with an interactive TUI for selection. Config-driven via YAML with support for multiple predefined flows.

## Requirements

1. **CLI interface** — `myagent [--flow <flow-name> <flow-args>] [--allowed-base <path>] [--config <path>] [--list-flows] [--check-config]`. Built with `clap` derive macros. If `--flow` is not provided, the app displays available flows and exits.

2. **YAML configuration** — Model settings, prompts, and flow definitions. Fields: `model`, `api_key` (or `OPENAI_API_KEY` env), `base_url` (optional), `prompts`, `flows` (map of flow name → flow configuration with system_prompt, user_prompt, tools, arguments).

**Config File Locations and Merging:**
- User config: `~/.config/myagent/config.yaml` (persistent across projects)
- Local config: `myagent.yaml` in current directory (project-specific)
- Explicit path: `--config <path>` (no merging, loads only from specified path)
- Merging: Local config overrides user config values; missing values fall back to user config
- Installation: `make install` or `make setup-config` creates config directory and copies example config (won't overwrite existing config)

3. **OpenAI client** — `async-openai` wrapper. Flow executor executes flows based on their configuration. Support custom `base_url`.

4. **Prompt templates** — Each flow defines its own `system_prompt` and `user_prompt`. Prompts use a proper template engine with Jinja2-like syntax (e.g., `{{ variable }}`, `{{ variable | filter }}`, `{% if condition %}`, `{% for item in items %}`). **Do NOT use blind string replacement** — use a proper template engine like `tera` or `minijinja` for safe and powerful templating with filters, conditionals, and loops.

5. **TUI multi-select** — After model calls multi_select tool, launch ratatui + crossterm TUI showing all items with checkboxes. Key bindings: ↑/↓ navigate, Space toggle, Enter confirm, q/ESC quit. Supports optional detail view with 'v' key.

6. **Flow definition** — Each flow contains:
   - `name`: Flow identifier
   - `description`: Human-readable description
   - `system_prompt`: System prompt for the model
   - `user_prompt`: User prompt template (can reference arguments)
   - `tools`: List of available tools for this flow
   - `arguments`: Flow-specific arguments definition

7. **Error handling** — `anyhow::Result` with `.context()`. User-friendly messages.

8. **Centralized logging** — All application messages use a centralized logging system configured via `config.yaml`. The configuration includes `logging.level` (trace/debug/info/warn/error). Use `tracing` crate with `tracing-subscriber` for structured logging. Messages use appropriate logging macros: `info!`, `warn!`, `error!`, `debug!`. Logs are output to stdout/stderr only, not to files. Format is hardcoded as human-readable text with level and message only. User-facing messages (help, validation results, flow output, errors) must always be printed using `println!`/`eprintln!` regardless of configured log level. Logging macros are used only for tool calls, debugging, and internal operations that should respect the log level filter.



9. **Dependencies** — `clap` (derive), `serde` + `serde_yaml`, `async-openai`, `ratatui` + `crossterm`, `tokio`, `regex`, `anyhow`, `minijinja`, `tracing`, `tracing-subscriber`, `globwalk`, `dirs`.

10. **Project structure** — `Cargo.toml`, `src/main.rs`, `src/client.rs`, `src/config.rs`, `src/flow.rs`, `src/tools/` (read_file, edit_file, create_file, delete_file, remove_dir, grep, list_dir, multi_select, git_status, git_diff, git_stage, git_commit), `src/tui.rs`, `src/types.rs`, `config.example.yaml`, `Makefile`.

11. **Security** — Path traversal prevention with `--allowed-base` flag. Canonicalize paths and verify they start with allowed base. Block `..` references. **Symlink safety**: All paths (including symlink targets) are canonicalized and verified to remain within the allowed base. Symlinks are not followed by default in file traversal to prevent directory traversal attacks.

12. **Tool logging** — All tool calls logged with timing information using the centralized logger. Execution flow: Model → Tool Call → Execute → Log → Response → Continue. Log at appropriate levels: `info!` for successful tool calls, `warn!` for recoverable issues, `error!` for failures.

13. **Model** — Uses `Intel/Qwen3.5-122B-A10B-int4-AutoRound` model.

14. **Prompt templating** — Uses MiniJinja template engine with Jinja2-like syntax supporting:
    - Variable substitution: `{{ variable }}`
    - Filters: `{{ variable | upper }}`, `{{ variable | trim }}`, `{{ variable | length }}`
    - Conditionals: `{% if condition %}...{% else %}...{% endif %}`
    - Loops: `{% for item in items %}...{% endfor %}`
    - Safe and powerful templating instead of blind string replacement

15. **Custom tools** — Users can define custom shell commands as tools in config.yaml under `custom_tools` key. Each custom tool:
    - Has a fixed command defined in config (no shell injection possible)
    - Accepts 3 optional arguments for output filtering (no defaults - returns all if not specified):
      - `head_lines` (integer): Number of first N lines to return
      - `tail_lines` (integer): Number of last N lines to return
      - `pattern` (string): Regex pattern to filter output
    - Executes in the allowed base directory
    - Supports configurable timeout (default: 60s)
    - Uses same logging pipeline as built-in tools
    - Post-processes output with head/tail/grep filters
    - The model can optionally use these arguments to filter large outputs
    - Example:
      ```yaml
      custom_tools:
        cargo_test:
          name: "cargo_test"
          command: "cargo test"
          description: "Run cargo tests and filter output"
          timeout: 60
      ```

16. **Prompt preview** — `--print-prompt` flag renders and prints both the system prompt and user prompt with all variable substitutions and exits. Useful for debugging template issues.

17. **Structured tool responses** — All tool calls return structured feedback in a format:
    ```
    called: <tool_name>
    status: success | error
    metadata: <metadata> (always included when applicable)
    === START ===
    [full output content]
    === END ===
    ```
    The format is consistent regardless of output size (single line or multi-line). Metadata provides context such as file size, line counts, or other relevant information. The START/END markers provide clear boundaries for the content, making it easy for the model to parse.

18. **Common system prompt** — A base system prompt that gets prepended to all flow-specific system prompts. Provides consistent behavior across all flows (e.g., "You are an AI assistant. Use tools appropriately. Return clear, concise responses."). Configurable in config.yaml.

19. **Edit conflict prevention** — When the model requests multiple edits to the same file in a single response, later edits may reference incorrect line numbers due to shifts from earlier edits. The system must detect and prevent such conflicts, requiring the model to re-read the file before making conflicting edits.

20. **Enhanced logging** — Tool calls log requested parameters at info level (file path and line ranges) and full parameters at debug level. Git operations include detailed error messages from stderr, stdout, or exit codes.

## Tools

Model-accessible tools:
- `read_file(file_path, start_line?, end_line?)` — Read file with optional line range (start_line and end_line are 1-indexed). **Maximum 2000 lines are returned; larger requests are automatically truncated**. Output format: `[line-number]content` with no extra spaces. Empty files return success with "[EMPTY FILE]" message, not an error. Metadata includes actual file size in bytes and indicates when content is truncated.
- `edit_file(file_path, start_line, end_line, new_text)` — Replace lines start_line to end_line (1-indexed, inclusive) with new_text
- `create_file(file_path, content?)` — Create a new file with optional content. Fails if file already exists. Creates parent directories if needed.
- `delete_file(file_path)` — Delete a file. Fails if file doesn't exist or is a directory.
- `remove_dir(dir_path)` — Remove an empty directory. Fails if directory doesn't exist or is a file.
- `grep(pattern, path?)` — Search files using a regex pattern. The `path` argument supports glob patterns (e.g., `src/**/*.rs`, `*.txt`) for flexible file matching. Searches are limited to 100,000 files and 100 directory levels. Symlinks are not followed for security. Path is validated against the allowed base to prevent directory traversal.
- `list_dir(dir_path)` — List directory
- `quiz(question, suggestions[], question_type)` — TUI multi-select for user input
- `git_status(path?)` — Get git status for repository or specific path. **Path is validated against the allowed base to prevent directory traversal**.
- `git_diff(path?, staged?)` — Get git diff (staged or unstaged). **Path is validated against the allowed base to prevent directory traversal**.
- `git_stage(file_path)` — Stage file(s) for commit. **Path is validated against the allowed base to prevent directory traversal**.
- `git_commit(message, title)` — Create commit with message and title

**Quiz Tool Details**:
- `question_type` parameter always set to `"multi_select"`
- `suggestions` array contains objects: `{id, description, suggested_fix}`
- TUI navigation: ↑/↓ navigate, Space toggle, Enter confirm, q/ESC quit

**Tool Logging**: All tools return `Result<String>` and are logged with timing.

## Flow Configuration

Flows are defined in config.yaml under the `flows` key. Each flow is a complete workflow with its own prompts and available tools.

### Flow Structure
```yaml
flows:
  <flow-name>:
    name: "<flow-name>"
    description: "<human-readable description>"
    system_prompt: |
      <system prompt for the model>
    user_prompt: |
      <user prompt template, can reference flow arguments>
    tools:
      - <tool-name-1>
      - <tool-name-2>
    arguments:
      - name: <arg-name>
        description: <description>
        required: true/false
```

### Example Flow: "review_and_fix"
```yaml
flows:
  review_and_fix:
    name: "review_and_fix"
    description: "Review file and apply fixes"
    system_prompt: |
      You are a code review assistant. Analyze the file and provide suggestions for improvements.
      Return your analysis and suggestions in structured format.
    user_prompt: |
      Please review the file at: {{file_path}}
    tools:
      - read_file
      - edit_file
      - create_file
      - delete_file
      - remove_dir
      - quiz
      - grep
      - list_dir
    arguments:
      - name: file_path
        description: "Path to the file to review"
        required: true
```

### Example Flow: "commit_changes"
```yaml
  commit_changes:
    name: "commit_changes"
    description: "Commit changes with AI-generated message"
    system_prompt: |
      You are a commit assistant. Analyze the git diff and create a meaningful commit message.
      The commit title should be concise (50 chars max) and the message should explain the changes.
    user_prompt: |
      Analyze the changes in: {{file_path}}
      Create a commit with a descriptive title and message.
    tools:
      - git_status
      - git_diff
      - git_stage
      - git_commit
      - read_file
      - create_file
      - delete_file
      - remove_dir
    arguments:
      - name: file_path
        description: "Path to the file to commit"
        required: false
```

## Flow Execution

### Flow Execution Model
Each flow is a self-contained workflow with:
1. **System prompt** - Defines the model's role and behavior
2. **User prompt** - Template that gets filled with flow arguments
3. **Available tools** - List of tools the model can use during this flow
4. **Arguments** - Flow-specific parameters passed by the user

### Execution Flow
1. Parse CLI arguments and flow selection
2. Load flow configuration from config file
3. Validate flow arguments against flow definition
4. Substitute variables in user prompt (`{{arg-name}}`)
5. Initialize OpenAI client with flow's system prompt
6. Execute flow with user prompt and available tools
7. Model can call any tool from the flow's available tools list
8. Return results to user

### Variable Substitution
User prompts support variable substitution:
- Flow arguments: `{{file_path}}`, `{{branch_name}}`, etc.
- Environment variables: `{{env.VAR_NAME}}`
- Default values: `{{arg-name:default}}`

## Verification
1. `cargo build` compiles
2. `myagent` (no arguments) displays available flows with descriptions
3. `myagent --flow review-and-fix src/main.rs --config config.yaml` works end-to-end
4. `myagent --flow commit-changes src/main.rs --config config.yaml` executes custom flow
5. `myagent --list-flows` shows available flows with descriptions (same as no args)
6. `myagent --check-config` validates config file and reports errors
7. TUI shows checkboxes, responds to Space/Enter/q
8. Fixes applied correctly
9. Config merging works: local overrides user config
10. `--allowed-base` restricts file access to specified directory
11. `cargo test` — tests pass for flow executor and new git tools
12. `make install` builds and sets up config
13. `make setup-config` creates config directory and copies example config (preserves existing config)


## Additional Notes
- Config is YAML with sensible defaults; API key can come from env var `OPENAI_API_KEY`
- File is modified in-place after user confirmation
- Flows are extensible - users can define custom flows in config
- **If no `--flow` flag is provided, the app displays available flows and exits**
- New flows can be added without code changes
- `--list-flows` displays all available flows with descriptions and arguments
- `--check-config` validates config structure and reports issues

