use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ABI {
    pub structs: HashMap<String, StructDescription>,
}

impl ABI {
    pub fn find_type(&self, name: &String) -> Option<StructDescription> {
        self.structs.get(name).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDescription {
    pub name: String,
    pub type_name: String,
}

#[derive(Debug, Clone, Eq)]
pub struct StructDescription {
    pub fields: Vec<FieldDescription>,
}

impl PartialEq for StructDescription {
    fn eq(&self, other: &Self) -> bool {
        self.fields.len() == other.fields.len()
            && self.fields.iter().all(|field| other.fields.contains(field))
    }
}

pub fn process_struct_definitions(
    node: &tree_sitter::Node,
    content: &str,
    file_path: &str,
) -> HashMap<String, StructDescription> {
    let mut struct_defs = HashMap::new();
    analyze_structs_recursive(&node, content, file_path, &mut struct_defs);
    struct_defs
}

fn analyze_structs_recursive(
    node: &tree_sitter::Node,
    content: &str,
    file_path: &str,
    struct_defs: &mut HashMap<String, StructDescription>,
) {
    for i in 0..node.child_count() {
        let child = node.child(i).unwrap();
        analyze_structs_recursive(&child, content, file_path, struct_defs);
    }

    if node.kind() != "struct_declaration" {
        return;
    }

    let Some(name_node) = node.child_by_field_name("name") else {
        return;
    };

    let struct_name = name_node
        .utf8_text(content.as_bytes())
        .unwrap_or("")
        .to_string();

    let mut fields = Vec::new();

    let Some(body_node) = node.child_by_field_name("body") else {
        return;
    };

    let mut cursor = body_node.walk();
    for child in body_node.children(&mut cursor) {
        if child.kind() == "struct_field_declaration" {
            let Some(field_name_node) = child.child_by_field_name("name") else {
                continue;
            };

            let Some(field_type_node) = child.child_by_field_name("type") else {
                continue;
            };

            let field_name = field_name_node
                .utf8_text(content.as_bytes())
                .unwrap_or("")
                .to_string();

            let field_type = field_type_node
                .utf8_text(content.as_bytes())
                .unwrap_or("")
                .to_string();

            fields.push(FieldDescription {
                name: field_name,
                type_name: field_type,
            });
        }
    }

    if !fields.is_empty() {
        struct_defs.insert(struct_name, StructDescription { fields });
    }
}
