//! Function extraction using tree-sitter.
//!
//! Parses source files to extract function/method boundaries.
//! Supported languages: C, C++, Python, Go, Rust, Java, Solidity.

use tree_sitter::{Language, Parser, Node};

// ── Language detection ───────────────────────────────────────────────────────

pub fn detect_language(file_path: &str) -> Option<&'static str> {
    let ext = file_path.rsplit('.').next()?;
    match ext {
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" | "hh" => Some("cpp"),
        "py" | "pyi" => Some("python"),
        "go" => Some("go"),
        "rs" => Some("rust"),
        "java" => Some("java"),
        // "sol" => Some("solidity"),  // TODO: re-enable with tree-sitter 0.25+
        _ => None,
    }
}

fn get_language(lang: &str) -> Option<Language> {
    match lang {
        "c" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "python" => Some(tree_sitter_python::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "rust" => Some(tree_sitter_rust::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        // "solidity" => Some(tree_sitter_solidity::LANGUAGE.into()),
        _ => None,
    }
}

// ── Function info ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize)]
pub struct FunctionInfo {
    pub name: String,
    pub kind: String,        // "function", "method", "class", "struct", "impl", "contract"
    pub start_line: usize,   // 1-indexed
    pub end_line: usize,     // 1-indexed, inclusive
    pub signature: String,   // first line of the function declaration
}

// ── Extraction ───────────────────────────────────────────────────────────────

/// Extract all functions/methods from source code.
pub fn extract_functions(file_path: &str, source: &str) -> Vec<FunctionInfo> {
    let lang_name = match detect_language(file_path) {
        Some(l) => l,
        None => return Vec::new(),
    };

    let language = match get_language(lang_name) {
        Some(l) => l,
        None => return Vec::new(),
    };

    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return Vec::new();
    }

    let tree = match parser.parse(source, None) {
        Some(t) => t,
        None => return Vec::new(),
    };

    let mut functions = Vec::new();
    let lines: Vec<&str> = source.lines().collect();
    let src_bytes = source.as_bytes();
    collect_functions(tree.root_node(), &lines, src_bytes, lang_name, &mut functions);
    functions
}

fn collect_functions(node: Node, lines: &[&str], src: &[u8], lang: &str, out: &mut Vec<FunctionInfo>) {
    let dominated = is_function_node(node.kind(), lang);
    if dominated {
        if let Some(func) = extract_function_info(node, lines, src, lang) {
            out.push(func);
        }
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_functions(child, lines, src, lang, out);
    }
}

fn is_function_node(kind: &str, lang: &str) -> bool {
    match lang {
        "c" => matches!(kind, "function_definition"),
        "cpp" => matches!(kind, "function_definition" | "template_declaration"),
        "python" => matches!(kind, "function_definition" | "class_definition"),
        "go" => matches!(kind, "function_declaration" | "method_declaration"),
        "rust" => matches!(kind, "function_item" | "impl_item"),
        "java" => matches!(kind, "method_declaration" | "constructor_declaration" | "class_declaration"),
        // "solidity" => matches!(kind, "function_definition" | "constructor_definition" | "modifier_definition" | "contract_declaration"),
        _ => false,
    }
}

fn extract_function_info(node: Node, lines: &[&str], src: &[u8], lang: &str) -> Option<FunctionInfo> {
    let start_line = node.start_position().row + 1; // 1-indexed
    let end_line = node.end_position().row + 1;

    // Extract name from child nodes
    let name = find_function_name(node, src, lang).unwrap_or_else(|| "<anonymous>".to_string());

    let kind = classify_node(node.kind(), lang);

    // Build signature from the first line(s) of the declaration
    let sig_line = if start_line <= lines.len() {
        lines[start_line - 1].trim().to_string()
    } else {
        String::new()
    };

    // Truncate long signatures
    let signature = if sig_line.len() > 200 {
        format!("{}...", &sig_line[..200])
    } else {
        sig_line
    };

    Some(FunctionInfo {
        name,
        kind: kind.to_string(),
        start_line,
        end_line,
        signature,
    })
}

fn find_function_name(node: Node, src: &[u8], lang: &str) -> Option<String> {
    // Look for known name child node kinds
    let name_kinds: &[&str] = match lang {
        "c" | "cpp" => &["declarator", "function_declarator"],
        "python" => &["identifier"],
        "go" => &["identifier", "field_identifier"],
        "rust" => &["identifier", "type_identifier"],
        "java" => &["identifier"],
        // "solidity" => &["identifier"],
        _ => return None,
    };

    // BFS for the name node (first match at shallowest depth)
    let mut queue = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        queue.push(child);
    }

    while let Some(n) = queue.first().cloned() {
        queue.remove(0);

        if name_kinds.contains(&n.kind()) {
            // For C/C++ function_declarator, dig deeper for the actual identifier
            if n.kind() == "function_declarator" || n.kind() == "declarator" {
                let mut inner_cursor = n.walk();
                for inner_child in n.children(&mut inner_cursor) {
                    if inner_child.kind() == "identifier" || inner_child.kind() == "qualified_identifier"
                        || inner_child.kind() == "field_identifier" || inner_child.kind() == "destructor_name"
                        || inner_child.kind() == "scoped_identifier"
                    {
                        return Some(inner_child.utf8_text(src).ok()?.to_string());
                    }
                    // For qualified names like ClassName::method
                    if inner_child.kind() == "qualified_identifier" || inner_child.kind() == "scoped_identifier" {
                        return Some(inner_child.utf8_text(src).ok()?.to_string());
                    }
                }
                // Fallback: use the declarator text itself
                let text = n.utf8_text(src).ok()?;
                // Extract just the name part before '('
                return Some(text.split('(').next()?.trim().to_string());
            }

            return Some(n.utf8_text(src).ok()?.to_string());
        }

        let mut child_cursor = n.walk();
        for child in n.children(&mut child_cursor) {
            queue.push(child);
        }
    }

    None
}

fn classify_node(kind: &str, lang: &str) -> &'static str {
    match (lang, kind) {
        (_, "class_definition") | (_, "class_declaration") => "class",
        (_, "impl_item") => "impl",
        // (_, "contract_declaration") => "contract",
        ("cpp", "template_declaration") => "template",
        (_, "constructor_declaration") | (_, "constructor_definition") => "constructor",
        (_, "modifier_definition") => "modifier",
        ("go", "method_declaration") => "method",
        ("java", "method_declaration") => "method",
        _ => "function",
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_language_by_extension() {
        assert_eq!(detect_language("src/main.rs"), Some("rust"));
        assert_eq!(detect_language("foo.cpp"), Some("cpp"));
        assert_eq!(detect_language("bar.h"), Some("c"));
        assert_eq!(detect_language("test.py"), Some("python"));
        assert_eq!(detect_language("main.go"), Some("go"));
        assert_eq!(detect_language("App.java"), Some("java"));
        // assert_eq!(detect_language("Token.sol"), Some("solidity")); // TODO: re-enable
        assert_eq!(detect_language("readme.md"), None);
    }

    #[test]
    fn extract_rust_functions() {
        let src = r#"
fn hello() {
    println!("hi");
}

fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#;
        let funcs = extract_functions("test.rs", src);
        assert_eq!(funcs.len(), 2);
        assert_eq!(funcs[0].name, "hello");
        assert_eq!(funcs[0].kind, "function");
        assert_eq!(funcs[1].name, "add");
    }

    #[test]
    fn extract_cpp_functions() {
        let src = r#"
void OpenLedger::accept(int x) {
    // body
}

bool OpenLedger::empty() const {
    return true;
}
"#;
        let funcs = extract_functions("test.cpp", src);
        assert!(funcs.len() >= 2);
    }

    #[test]
    fn extract_python_functions() {
        let src = r#"
def hello():
    print("hi")

class MyClass:
    def method(self):
        pass
"#;
        let funcs = extract_functions("test.py", src);
        assert!(funcs.len() >= 2); // hello + MyClass (class_definition)
    }

    #[test]
    fn extract_go_functions() {
        let src = r#"
package main

func hello() {
    fmt.Println("hi")
}

func (s *Server) Handle(w http.ResponseWriter, r *http.Request) {
}
"#;
        let funcs = extract_functions("test.go", src);
        assert!(funcs.len() >= 2);
    }

    #[test]
    fn extract_java_functions() {
        let src = r#"
public class App {
    public void hello() {
        System.out.println("hi");
    }
    public int add(int a, int b) {
        return a + b;
    }
}
"#;
        let funcs = extract_functions("App.java", src);
        assert!(funcs.len() >= 2); // class + methods
    }

    #[test]
    fn unsupported_extension_returns_empty() {
        let funcs = extract_functions("readme.md", "# hello");
        assert!(funcs.is_empty());
    }

    #[test]
    fn empty_source_returns_empty() {
        let funcs = extract_functions("test.rs", "");
        assert!(funcs.is_empty());
    }
}
