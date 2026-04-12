use minijinja::Environment;
use std::collections::HashMap;

/// Renders a prompt template using MiniJinja with Jinja2-like syntax
/// Supports: {{ variable }}, {{ variable | filter }}, {% if condition %}, {% for item in items %}
pub fn render_prompt(template: &str, variables: &HashMap<String, String>) -> Result<String, minijinja::Error> {
    let mut env = Environment::new();
    
    // Add custom filters
    env.add_filter("upper", |value: String| value.to_uppercase());
    env.add_filter("lower", |value: String| value.to_lowercase());
    env.add_filter("trim", |value: String| value.trim().to_string());
    env.add_filter("length", |value: String| value.len());
    
    // Create context from variables
    let mut ctx = std::collections::HashMap::new();
    for (key, value) in variables {
        ctx.insert(key, value);
    }
    
    // Render the template
    let result = env.render_str(template, ctx);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_simple() {
        let mut vars = HashMap::new();
        vars.insert("file_path".to_string(), "test.rs".to_string());
        
        let template = "Please review: {{ file_path }}";
        let result = render_prompt(template, &vars).unwrap();
        
        assert_eq!(result, "Please review: test.rs");
    }

    #[test]
    fn test_render_with_filter() {
        let mut vars = HashMap::new();
        vars.insert("filename".to_string(), "test.rs".to_string());
        
        let template = "File: {{ filename | upper }}";
        let result = render_prompt(template, &vars).unwrap();
        
        assert_eq!(result, "File: TEST.RS");
    }

    #[test]
    fn test_render_with_conditionals() {
        let mut vars = HashMap::new();
        vars.insert("show_details".to_string(), "true".to_string());
        
        let template = "{% if show_details == 'true' %}Detailed view{% else %}Brief view{% endif %}";
        let result = render_prompt(template, &vars).unwrap();
        
        assert_eq!(result, "Detailed view");
    }
}
