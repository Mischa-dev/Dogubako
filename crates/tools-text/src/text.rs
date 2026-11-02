use dogubako_core::prelude::*;

pub fn tools() -> Vec<Arc<dyn Tool>> {
    vec![change_case()]
}

fn change_case() -> Arc<dyn Tool> {
    let spec = ToolSpec::new("text.case", "Change Case", Category::Text)
        .describe("Convert text to UPPER, lower, Title, Sentence, camelCase, snake_case, kebab-case and more")
        .keywords(&["uppercase", "lowercase", "title case", "camel", "snake", "kebab"])
        .param(ParamSpec::textarea("text").label("Text").required().primary())
        .param(
            ParamSpec::choice(
                "case",
                &[("upper", "UPPER CASE"), ("lower", "lower case"), ("title", "Title Case"), ("sentence", "Sentence case"), ("camel", "camelCase"), ("pascal", "PascalCase"), ("snake", "snake_case"), ("kebab", "kebab-case"), ("constant", "CONSTANT_CASE"), ("invert", "iNVERT cASE")],
            )
            .default_str("upper"),
        )
        .text_output("text/plain");
    sync_tool(spec, |_ctx, p| {
        let text = p.text("text");
        let out = match p.str("case")? {
            "upper" => text.to_uppercase(),
            "lower" => text.to_lowercase(),
            "invert" => text.chars().map(|c| if c.is_uppercase() { c.to_lowercase().collect::<String>() } else { c.to_uppercase().collect() }).collect(),
            other => return Err(ToolError::invalid("case", format!("{other} not implemented in example"))),
        };
        Ok(ToolOutput::text(out))
    })
}

#[cfg(test)]
mod tests {
    use dogubako_core::testing::TestHarness;
    use serde_json::json;

    #[tokio::test]
    async fn upper() {
        let h = TestHarness::new(super::tools());
        let out = h.run("text.case", json!({"text": "Hello", "case": "upper"})).await.unwrap();
        assert_eq!(out.text.as_deref(), Some("HELLO"));
        let err = h.run("text.case", json!({"text": "x", "case": "nope"})).await.unwrap_err();
        assert_eq!(err.code(), "invalid_input");
        let err = h.run("text.case", json!({"bogus": 1})).await.unwrap_err();
        assert_eq!(err.code(), "invalid_input");
    }
}
