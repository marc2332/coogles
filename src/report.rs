use serde::Serialize;

#[derive(Default, Serialize)]
pub struct Metrics {
    pub rust_files: u64,
    pub code_loc: u64,
    pub test_code_loc: u64,
    pub comment_loc: u64,
    pub doc_loc: u64,
    pub inner_doc_loc: u64,
    pub outer_doc_loc: u64,
    pub block_comment_loc: u64,
    pub block_doc_loc: u64,
    pub example_code_loc: u64,
}

#[derive(Serialize)]
pub struct Snapshot {
    pub commit: String,
    pub offset: usize,
    pub timestamp: i64,
    pub metrics: Metrics,
    pub parse_errors: Vec<String>,
}

pub fn html(json: &str) -> String {
    let escaped = json
        .replace('&', "\\u0026")
        .replace('<', "\\u003c")
        .replace('>', "\\u003e");
    include_str!("viewer.html").replace("__REPORT_JSON__", &escaped)
}

#[cfg(test)]
mod tests {
    use crate::report::html;

    #[test]
    fn escapes_script_closers_in_embedded_json() {
        let page = html(r#"{"repository":"</script><script>alert(1)</script>"}"#);
        assert!(page.contains(r#"\u003c/script\u003e"#));
        assert!(!page.contains("<script>alert(1)"));
        assert!(!page.contains("__REPORT_JSON__"));
    }
}
